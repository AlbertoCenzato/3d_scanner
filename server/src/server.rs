use std::net::{SocketAddr, TcpStream};

use crate::scanner;
use websocket::sync::{Client, Server};

pub fn run_websocket_server(port: u16, scanner: &mut scanner::Scanner) -> anyhow::Result<()> {
    println!("Starting WebSocket server...");
    let connection_string = format!("0.0.0.0:{}", port);
    let server = Server::bind(connection_string)?;
    println!(
        "WebSocket server listening for incoming connections on port {}",
        port
    );

    for request in server.filter_map(Result::ok) {
        if let Ok(client) = request.accept() {
            let peer_address = client.peer_addr()?;
            println!("Opened connection with {}", peer_address);
            let res = handle_connection(client, &peer_address, scanner);
            match res {
                Ok(_) => println!("Closed connection with {}", peer_address),
                Err(e) => println!("Error: {} - connection with {} closed", e, peer_address),
            }

            println!(
                "WebSocket server listening for incoming connections on port {}",
                port
            );
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
    for message in receiver.incoming_messages() {
        let message = message?;
        println!("Received message: {:?}", message);
        match message {
            websocket::OwnedMessage::Close(_) => {
                let message = websocket::Message::close();
                sender.send_message(&message).unwrap();
                println!("Client {} disconnected", peer_address);
                break;
            }
            websocket::OwnedMessage::Ping(ping) => {
                let message = websocket::Message::pong(ping);
                sender.send_message(&message).unwrap();
                println!("Ponged");
            }
            websocket::OwnedMessage::Text(text) => {
                println!("Text message received: {}", text);
                let message: msg::command::Command = serde_json::from_str(&text)?;
                let reply = process_message(message, scanner)?;
                sender.send_message(&reply)?;
            }
            websocket::OwnedMessage::Binary(_) => {
                println!("Binary message received, not supported");
            }
            _ => {
                println!("Unsupported message type");
            }
        };
    }
    return Ok(());
}

fn process_message(
    command: msg::command::Command,
    scanner: &mut scanner::Scanner,
) -> anyhow::Result<websocket::OwnedMessage> {
    use msg::command::Command as cmd;
    match command {
        cmd::Status => status(scanner),
        cmd::Replay => replay(scanner),
    }
}

fn status(scanner: &scanner::Scanner) -> anyhow::Result<websocket::OwnedMessage> {
    let status = scanner.status();
    let response = serde_json::to_string(&status)?;
    let message = websocket::OwnedMessage::Text(response);
    return Ok(message);
}

fn replay(_scanner: &mut scanner::Scanner) -> anyhow::Result<websocket::OwnedMessage> {
    //let result = scanner.start(sender);

    let response = msg::response::Response::Ok;
    let response = serde_json::to_string(&response)?;
    let message = websocket::OwnedMessage::Text(response);
    return Ok(message);
}
