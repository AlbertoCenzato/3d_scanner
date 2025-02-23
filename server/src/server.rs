use crate::scanner;
use websocket::sync::Server;

pub fn run_websocket_server(port: u16, scanner: &mut scanner::Scanner) -> anyhow::Result<()> {
    println!("Starting WebSocket server...");
    let connection_string = format!("0.0.0.0:{}", port);
    let server = Server::bind(connection_string)?;
    println!("WebSocket server listening on port {}", port);

    for request in server.filter_map(Result::ok) {
        if let Ok(client) = request.accept() {
            let peer_address = client.peer_addr()?;
            println!("Connection from {}", peer_address);

            let (mut receiver, mut sender) = client.split()?;
            for message in receiver.incoming_messages() {
                if let Err(e) = message {
                    println!("Error: {:?}", e);
                    break;
                }

                let message = message.unwrap();
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
                        process_message(message, scanner)?;
                    }
                    websocket::OwnedMessage::Binary(_) => {
                        println!("Binary message received, not supported");
                    }
                    _ => {
                        println!("Unsupported message type");
                    }
                };
            }
            println!("Listening for incoming connections...");
        }
    }

    return Ok(());
}

fn process_message(
    command: msg::command::Command,
    scanner: &mut scanner::Scanner,
) -> anyhow::Result<websocket::Message> {
    use msg::command::Command as cmd;
    match command {
        cmd::Status => status(scanner),
        cmd::Replay => replay(scanner),
    }
}

fn status(scanner: &scanner::Scanner) -> anyhow::Result<websocket::Message> {
    let status = scanner.status();
    let response = serde_json::to_string(&status)?;
    let message = websocket::Message::text(response);
    return Ok(message);
}

fn replay(scanner: &mut scanner::Scanner) -> anyhow::Result<websocket::Message> {
    // initialize a websocket client
    let url = websocket::url::Url::parse("wss://echo.websocket.org/")?;
    let client = websocket::ClientBuilder::from_url(&url).connect_insecure()?;
    let (_, sender) = client.split()?;

    let result = scanner.start(sender);
    let response = match result {
        Ok(_) => msg::response::Response::Ok,
        Err(_) => msg::response::Response::Error,
    };
    let response = serde_json::to_string(&response)?;
    let message = websocket::Message::text(response);
    return Ok(message);
}
