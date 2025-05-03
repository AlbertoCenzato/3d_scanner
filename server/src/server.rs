use crate::scanner;
use msg::response::Response;
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use websocket::sync::{Client, Server};

pub fn run_websocket_server(port: u16, scanner: &mut scanner::Scanner) -> anyhow::Result<()> {
    println!("Starting WebSocket server...");
    let connection_string = format!("0.0.0.0:{port}");
    let server = Server::bind(connection_string)?;
    println!("WebSocket server listening for incoming connections on port {port}");

    for req in server {
        println!("Incoming connection request");
        match req {
            Ok(request) => {
                println!("Ok request");
                match request.accept() {
                    Ok(client) => {
                        println!("Accepted connection request");
                        let peer_address = client.peer_addr()?;
                        println!("Opened connection with {}", peer_address);
                        let res = handle_connection(client, &peer_address, scanner);
                        match res {
                            Ok(_) => println!("Closed connection with {peer_address}"),
                            Err(e) => {
                                println!("Error: {e} - connection with {peer_address} closed")
                            }
                        }

                        println!(
                            "WebSocket server listening for incoming connections on port {}",
                            port
                        );
                    }
                    Err((stream, e)) => {
                        println!("Failed to accept connection: {e}");
                        let addr = stream.peer_addr()?;
                        println!("Failed to accept connection from {addr}: {e}");
                    }
                }
            }
            Err(invalid_connection) => {
                let e = &invalid_connection.error;
                println!("Failed to accept connection: {e}");
                continue;
            }
        }
    }

    return Ok(());
}

fn handle_connection(
    client: Client<TcpStream>,
    peer_address: &SocketAddr,
    scanner: &mut scanner::Scanner,
) -> anyhow::Result<()> {
    let (mut receiver, mut sender) = client.split()?;
    let (send_msg, outgoing_msgs) = mpsc::channel::<Response>();

    let sender_thread = std::thread::spawn(move || {
        println!("Sender thread started");
        for response in outgoing_msgs {
            let owned_message =
                serde_json::to_string(&response).map(|s| websocket::OwnedMessage::Text(s));

            match owned_message {
                Ok(message) => {
                    println!("Sending message: {:?}", message);
                    if let Err(e) = sender.send_message(&message) {
                        println!("Failed to send message: {e}");
                    }
                }
                Err(e) => {
                    println!("Failed to create message: {e}");
                }
            };
        }
        println!("Sender thread finished");
    });

    for message in receiver.incoming_messages() {
        let message = message?;
        println!("Received message: {:?}", message);
        match message {
            websocket::OwnedMessage::Close(_) => {
                send_msg.send(Response::Close)?;
                println!("Client {} disconnected", peer_address);
                break;
            }
            websocket::OwnedMessage::Text(text) => {
                println!("Text message received: {}", text);
                let message: msg::command::Command = serde_json::from_str(&text)?;
                let reply = process_message(message, scanner, &send_msg)?;
                send_msg.send(reply)?;
            }
            websocket::OwnedMessage::Binary(_) => {
                println!("Binary message received, not supported");
            }
            _ => {
                println!("Unsupported message type");
            }
        };
    }
    sender_thread.join().unwrap();
    return Ok(());
}

fn process_message(
    command: msg::command::Command,
    scanner: &mut scanner::Scanner,
    sender: &mpsc::Sender<Response>,
) -> anyhow::Result<Response> {
    use msg::command::Command as cmd;
    match command {
        cmd::Status => Ok(Response::Status(scanner.status())),
        cmd::Replay => replay(scanner, sender.clone()),
    }
}

fn replay(
    scanner: &mut scanner::Scanner,
    sender: mpsc::Sender<Response>,
) -> anyhow::Result<Response> {
    println!("Replay command received. Starting replay...");
    let result = scanner.start(sender);
    return Ok(msg::response::Response::Ok);
}
