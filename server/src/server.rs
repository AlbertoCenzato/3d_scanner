use crate::scanner::{IdleState, RunningState, Scanner};
use log::{error, info, warn};
use msg::command::Command;
use msg::response::Response;
use std::net::{SocketAddr, TcpStream};
use std::sync::{mpsc, Arc};
use tungstenite;

enum ActiveScanner {
    Idle(Scanner<IdleState>),
    Running(Scanner<RunningState>),
}

pub fn run_websocket_server(port: u16, scanner: Scanner<IdleState>) -> anyhow::Result<()> {
    let mut scanner = scanner;

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

        scanner = handle_connection(client, &addr, scanner);
    }

    return Ok(());
}

fn handle_connection(
    connection: tungstenite::WebSocket<TcpStream>,
    peer_address: &SocketAddr,
    scanner: Scanner<IdleState>,
) -> Scanner<IdleState> {
    let mut scanner = ActiveScanner::Idle(scanner);
    let connection = Arc::new(std::sync::Mutex::new(connection));
    let receiver = connection.clone();
    let sender = connection.clone();

    let (send_msg, outgoing_msgs) = mpsc::channel::<Response>();

    let sender_thread = std::thread::spawn(move || {
        info!("Sender thread started");
        for msg in outgoing_msgs {
            let data = msg.to_bytes().into();
            let msg = tungstenite::Message::Binary(data);
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
        let res = match message {
            tungstenite::Message::Close(_) => {
                info!("Client {peer_address} requested disconnection");
                (scanner, Response::Close)
            }
            tungstenite::Message::Text(text) => {
                info!("Text message received: {text}");
                let error = Response::Error("Text messages are not supported".to_string());
                (scanner, error)
            }
            tungstenite::Message::Binary(bytes) => match Command::from_bytes(&bytes) {
                Ok(command) => process_message(command, scanner, &send_msg),
                Err(e) => {
                    error!("Failed to parse command: {e}");
                    let error = Response::Error(format!("Invalid command: {e}"));
                    (scanner, error)
                }
            },
            _ => {
                warn!("Unsupported message type");
                let error = Response::Error("Unsupported message type".to_string());
                (scanner, error)
            }
        };

        scanner = res.0;
        let response = res.1;

        let res = send_msg.send(response);
        if let Err(e) = res {
            error!("Internal send queue broken: {e}");
            error!("Closing connection with {peer_address}");
            break;
        }
    }

    drop(send_msg); // Close the sender channel to stop the sender thread
    sender_thread.join().expect("Failed to join sender thread");

    let scanner = match scanner {
        ActiveScanner::Idle(s) => s,
        ActiveScanner::Running(r) => {
            let (s, res) = r.stop();
            if let Err(e) = res {
                error!("Failed to stop scanner: {e}");
            }
            s
        }
    };

    return scanner;
}

fn process_message(
    command: msg::command::Command,
    scanner: ActiveScanner,
    sender: &mpsc::Sender<Response>,
) -> (ActiveScanner, Response) {
    use msg::command::Command as cmd;
    let (scanner, response) = match command {
        cmd::Status => (scanner, Err("Status not implemented".to_string())),
        cmd::Replay => match scanner {
            ActiveScanner::Idle(s) => (ActiveScanner::Running(s.start(sender.clone())), Ok(())),
            ActiveScanner::Running(r) => (
                ActiveScanner::Running(r),
                Err("Acquisition already in progress".to_string()),
            ),
        },
        cmd::Stop => match scanner {
            ActiveScanner::Idle(s) => {
                log::info!("Scanner already stopped");
                (ActiveScanner::Idle(s), Ok(()))
            }
            ActiveScanner::Running(r) => {
                let (s, res) = r.stop();
                if let Err(e) = res {
                    error!("Failed to stop scanner: {e}");
                }
                (ActiveScanner::Idle(s), Ok(()))
            }
        },
    };

    let response = match response {
        Ok(()) => Response::Ok,
        Err(error) => Response::Error(format!("Error processing command {command:?}: {error}")),
    };
    return (scanner, response);
}
