use crate::scanner;
use log::{error, info, warn};
use msg::response::Response;
use std::net::{SocketAddr, TcpStream};
use std::sync::{mpsc, Arc};
use tungstenite;

pub fn run_websocket_server(port: u16, scanner: &mut scanner::Scanner) -> anyhow::Result<()> {
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
        let client = match tungstenite::accept(stream) {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to accept WebSocket connection: {e}");
                continue;
            }
        };

        info!("WebSocket client connected: {}", addr);

        match handle_connection(client, &addr, scanner) {
            Ok(_) => info!("Closed connection with {addr}"),
            Err(e) => {
                error!("Error: {e} - connection with {addr} closed")
            }
        }
    }

    return Ok(());
}

fn handle_connection(
    connection: tungstenite::WebSocket<TcpStream>,
    peer_address: &SocketAddr,
    scanner: &mut scanner::Scanner,
) -> anyhow::Result<()> {
    let connection = Arc::new(std::sync::Mutex::new(connection));
    let receiver = connection.clone();
    let sender = connection.clone();

    let (send_msg, outgoing_msgs) = mpsc::channel::<Response>();

    let sender_thread = std::thread::spawn(move || {
        info!("Sender thread started");
        for msg in outgoing_msgs {
            let msg = match serde_json::to_string(&msg) {
                Ok(s) => {
                    info!("Serialized message: {s}");
                    tungstenite::Message::text(s)
                }
                Err(e) => {
                    error!("Failed to serialize message: {e}");
                    continue;
                }
            };

            let mut sender = sender.lock().unwrap();
            if let Err(e) = sender.write(msg) {
                error!("Failed to send message: {e}");
            }
        }
        info!("Sender thread finished");
    });

    loop {
        let message_res = receiver.lock().unwrap().read();
        let message = match message_res {
            Ok(msg) => msg,
            Err(e) => match e {
                tungstenite::Error::ConnectionClosed => {
                    info!("Connection closed by client: {peer_address}");
                    break;
                }
                _ => {
                    error!("Error reading message from client {peer_address}: {e}");
                    error!("Closing connection with {peer_address}");
                    break;
                }
            },
        };

        info!("Received message: {:?}", message);
        let response = match message {
            tungstenite::Message::Close(_) => {
                info!("Client {peer_address} requested disconnection");
                Response::Close
            }
            tungstenite::Message::Text(text) => {
                info!("Text message received: {text}");
                match serde_json::from_str(&text) {
                    Ok(command) => process_message(command, scanner, &send_msg),
                    Err(e) => {
                        error!("Failed to parse command: {e}");
                        Response::Error(format!("Invalid command: {e}"))
                    }
                }
            }
            tungstenite::Message::Binary(_) => {
                warn!("Binary message received, not supported");
                Response::Error("Binary messages are not supported".to_string())
            }
            _ => {
                warn!("Unsupported message type");
                Response::Error("Unsupported message type".to_string())
            }
        };

        let res = send_msg.send(response);
        if let Err(e) = res {
            error!("Internal send queue broken: {e}");
            error!("Closing connection with {peer_address}");
            break;
        }
    }

    drop(send_msg); // Close the sender channel to stop the sender thread
    sender_thread.join().expect("Failed to join sender thread");
    return Ok(());
}

fn process_message(
    command: msg::command::Command,
    scanner: &mut scanner::Scanner,
    sender: &mpsc::Sender<Response>,
) -> Response {
    use msg::command::Command as cmd;
    let response = match command {
        cmd::Status => Ok(Response::Status(scanner.status())),
        cmd::Replay => replay(scanner, sender.clone()),
    };

    match response {
        Ok(res) => res,
        Err(e) => Response::Error(format!("Error processing command {command:?}: {e}")),
    }
}

fn replay(
    scanner: &mut scanner::Scanner,
    sender: mpsc::Sender<Response>,
) -> anyhow::Result<Response> {
    info!("Replay command received. Starting replay...");
    scanner.start(sender)?;
    return Ok(msg::response::Response::Ok);
}
