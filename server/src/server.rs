use crate::scanner::{Scanner, ScannerError};
use log::{debug, error, info, warn};
use msg::command::Command;
use msg::response::Response;
use std::net::TcpStream;
use std::sync::mpsc;
use tungstenite;

pub fn run_websocket_server(port: u16, scanner: &mut Scanner) -> anyhow::Result<()> {
    info!("Starting WebSocket server...");
    let connection_string = format!("0.0.0.0:{port}");
    let server = std::net::TcpListener::bind(connection_string)?;
    info!("WebSocket server listening for incoming connections on port {port}");

    for stream in server.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(e) => {
                error!("Failed to accept connection: {e}");
                continue;
            }
        };

        let addr = match stream.peer_addr() {
            Ok(addr) => addr,
            Err(e) => {
                error!("Failed to get peer address: {e}");
                continue;
            }
        };

        info!("New connection from {}", addr);
        let res = stream.set_nonblocking(true);
        if let Err(e) = res {
            error!("Failed to set tcp stream in non-blocking mode, dropping connection: {e}");
            continue;
        }

        let config = tungstenite::protocol::WebSocketConfig::default().write_buffer_size(0);
        let client = match tungstenite::accept_with_config(stream, Some(config)) {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to accept WebSocket connection: {e}");
                continue;
            }
        };

        info!("WebSocket client connected: {addr}");
        handle_connection(client, scanner);
        info!("WebSocket client disconnected: {addr}");
    }

    return Ok(());
}

/// # Warning
/// This function expects the provided WebSocket to be in **non-blocking mode**.
/// If the socket is blocking, the loop may hang or not behave as intended.
fn handle_connection(mut connection: tungstenite::WebSocket<TcpStream>, scanner: &mut Scanner) {
    let (send_msg, outgoing_msgs) = mpsc::channel::<Response>();

    loop {
        //info!("Reading incoming messages");
        let result = receive(&mut connection, scanner, &send_msg);
        if let Err(e) = result {
            error!("Failed to receive message: {e}");
            break;
        }

        //info!("Sending outgoing messages");
        send(&mut connection, &outgoing_msgs);

        // sleep for a short duration to avoid busy-waiting
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    if let Err(e) = scanner.stop() {
        error!("Failed to stop scanner: {e}");
    }
}

fn receive(
    websocket: &mut tungstenite::WebSocket<std::net::TcpStream>,
    scanner: &mut Scanner,
    outbound_queue: &mpsc::Sender<Response>,
) -> Result<(), tungstenite::Error> {
    let message = match websocket.read() {
        Ok(msg) => msg,
        Err(e) => match e {
            tungstenite::Error::Io(io_err) if std::io::ErrorKind::WouldBlock == io_err.kind() => {
                // no message ready to be read yet
                return Ok(());
            }
            _ => {
                return Err(e);
            }
        },
    };

    debug!("Received message: {:?}", message);
    let res = match message {
        tungstenite::Message::Close(_) => {
            info!("Client requested disconnection");
            Response::Close
        }
        tungstenite::Message::Text(text) => {
            info!("Text message received: {text}");
            Response::Error("Text messages are not supported".to_string())
        }
        tungstenite::Message::Binary(bytes) => match Command::from_bytes(&bytes) {
            Ok(command) => process_message(command, scanner, &outbound_queue),
            Err(e) => {
                error!("Failed to parse command: {e}");
                Response::Error(format!("Invalid command: {e}"))
            }
        },
        _ => {
            warn!("Unsupported message type");
            Response::Error("Unsupported message type".to_string())
        }
    };

    let res = outbound_queue.send(res);
    if let Err(e) = res {
        error!("Internal send queue broken: {e}");
    }

    return Ok(());
}

fn send(
    websocket: &mut tungstenite::WebSocket<TcpStream>,
    outbound_queue: &mpsc::Receiver<Response>,
) {
    for msg in outbound_queue.try_iter() {
        let data: tungstenite::Bytes = msg.to_bytes().into();
        log::debug!("Sending {} bytes", data.len());
        let msg = tungstenite::Message::Binary(data);
        if let Err(e) = websocket.write(msg) {
            error!("Failed to send message: {e}");
        }
    }
}

fn process_message(
    command: msg::command::Command,
    scanner: &mut Scanner,
    sender: &mpsc::Sender<Response>,
) -> Response {
    use msg::command::Command as cmd;
    let response = match command {
        cmd::Status => Err(ScannerError::CommandNotImplemented(cmd::Status.to_string())),
        cmd::Replay => scanner.start(sender.clone()),
        cmd::Stop => scanner.stop(),
    };

    let response = match response {
        Ok(()) => Response::Ok,
        Err(error) => Response::Error(format!("Error processing command {command:?}: {error}")),
    };
    return response;
}
