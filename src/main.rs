#[macro_use]
extern crate serde_derive;
extern crate chrono;


use std::fs::File;
use std::io::{Read, Write};


use futures::prelude::*;
use chrono::prelude::*;

use libp2p::{
    identity,
    tokio_codec::{FramedRead, LinesCodec},
    NetworkBehaviour, PeerId, Swarm,
};
mod blockchain;

use blockchain::*;

// We create a custom network behaviour that combines floodsub and mDNS.
    // In the future, we want to improve libp2p to make this easier to do.
    #[derive(NetworkBehaviour)]
    struct MyBehaviour<TSubstream: libp2p::tokio_io::AsyncRead + libp2p::tokio_io::AsyncWrite> {
        floodsub: libp2p::floodsub::Floodsub<TSubstream>,
        mdns: libp2p::mdns::Mdns<TSubstream>,
    }

    impl<TSubstream: libp2p::tokio_io::AsyncRead + libp2p::tokio_io::AsyncWrite>
        libp2p::swarm::NetworkBehaviourEventProcess<libp2p::mdns::MdnsEvent>
        for MyBehaviour<TSubstream>
    {
        fn inject_event(&mut self, event: libp2p::mdns::MdnsEvent) {
            match event {
                libp2p::mdns::MdnsEvent::Discovered(list) => {
                    for (peer, _) in list {
                        self.floodsub.add_node_to_partial_view(peer);
                    }
                }
                libp2p::mdns::MdnsEvent::Expired(list) => {
                    for (peer, _) in list {
                        if !self.mdns.has_node(&peer) {
                            self.floodsub.remove_node_from_partial_view(&peer);
                        }
                    }
                }
            }
        }
    }

    impl<TSubstream: libp2p::tokio_io::AsyncRead + libp2p::tokio_io::AsyncWrite>
        libp2p::swarm::NetworkBehaviourEventProcess<libp2p::floodsub::FloodsubEvent>
        for MyBehaviour<TSubstream>
    {
        // Called when `floodsub` produces an event.
        fn inject_event(&mut self, message: libp2p::floodsub::FloodsubEvent) {
            if let libp2p::floodsub::FloodsubEvent::Message(message) = message {
                println!(
                    "Received: '{:?}' from {:?}",
                    String::from_utf8_lossy(&message.data),
                    message.source
                );
            
                // receive new transaction from P2P event stream , mine a new block and append it to blockchain
            //let _new_block = process_new_transaction(&mut String::from_utf8_lossy(&message.data));
            process_new_block(&mut String::from_utf8_lossy(&message.data));
            }
        }
    }

fn process_new_block(new_block_text: &str) {

    let mut file = File::open("Blockchain.json").expect("Unable to open");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Unable to read file");

    let mut p2p_bc: Vec<Block> = serde_json::from_str(&contents).unwrap();
    let new_block: Block = serde_json::from_str(new_block_text).expect("unable to deserialize new block.");

    println!(
        " New block received after deserialize is {:?}. ",
        new_block
    );
    p2p_bc.push(new_block);

    let mut file2 = File::create("Blockchain.json").expect("Unable to write file");
    file2
        .write_all(serde_json::to_string(&p2p_bc).unwrap().as_bytes())
        .expect("Unable to write to file");

    //File::create("Blockchain.json").expect("Unable to write file");

    for block in p2p_bc.iter() {
        println!("Block for index {} is {}", block.block_number, block.serialize_block());
    }

}

fn process_new_transaction(new_txn_text: &str) -> String {
    println!("Recvd in process_block {}", new_txn_text);

    let mut file = File::open("Blockchain.json").expect("Unable to open");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Unable to read file");

    let mut p2p_bc: Vec<Block> = serde_json::from_str(&contents).unwrap();

    println!(
        " {}. Please enter transaction details and press enter",
        p2p_bc.len()
    );

    let new_txn = Transaction {
        transaction_id: String::from("1"),
        transaction_timestamp: Utc::now().timestamp(),
        transaction_details: String::from(new_txn_text),
    };
    let mut new_block = Block::new(vec![new_txn], &p2p_bc[p2p_bc.len() - 1]);
    

    Block::mine_new_block(&mut new_block, &PREFIX);
    let new_block_clone = new_block.clone();
    p2p_bc.push(new_block);

    let mut file2 = File::create("Blockchain.json").expect("Unable to write file");
    file2
        .write_all(serde_json::to_string(&p2p_bc).unwrap().as_bytes())
        .expect("Unable to write to file");

    //File::create("Blockchain.json").expect("Unable to write file");

    for block in p2p_bc.iter() {
        println!("Block for index {} is {}", block.block_number, block.serialize_block());
    }
    serde_json::to_string(&new_block_clone).expect("Unable to serialize")
}

fn main() {
    println!("Welcome to P2P Rust Blockchain experiment");

    //create blockchain
    let p2p_bc: Vec<Block> = vec![Block::genesis()];

    let mut file = File::create("Blockchain.json").expect("Unable to write file");
    file.write_all(serde_json::to_string(&p2p_bc).unwrap().as_bytes())
        .expect("Unable to write to file");

    // Create a random PeerId
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::build_development_transport(local_key);

    // Create a Floodsub topic
    let floodsub_topic = libp2p::floodsub::TopicBuilder::new("chat").build();

    

    // Create a Swarm to manage peers and events
    let mut swarm = {
        let mut behaviour = MyBehaviour {
            floodsub: libp2p::floodsub::Floodsub::new(local_peer_id.clone()),
            mdns: libp2p::mdns::Mdns::new().expect("Failed to create mDNS service"),
        };

        behaviour.floodsub.subscribe(floodsub_topic.clone());
        libp2p::Swarm::new(transport, behaviour, local_peer_id)
    };


    // Reach out to another node if specified
    if let Some(to_dial) = std::env::args().nth(1) {
        let dialing = to_dial.clone();
        match to_dial.parse() {
            Ok(to_dial) => match libp2p::Swarm::dial_addr(&mut swarm, to_dial) {
                Ok(_) => println!("Dialed {:?}", dialing),
                Err(e) => println!("Dial {:?} failed: {:?}", dialing, e),
            },
            Err(err) => println!("Failed to parse address to dial: {:?}", err),
        }
    }

    // Read full lines from stdin
    let stdin = tokio_stdin_stdout::stdin(0);
    let mut framed_stdin = FramedRead::new(stdin, LinesCodec::new());

    // Listen on all interfaces and whatever port the OS assigns
    libp2p::Swarm::listen_on(&mut swarm, "/ip4/0.0.0.0/tcp/0".parse().unwrap()).unwrap();

    // Kick it off
    let mut listening = false;
    tokio::run(futures::future::poll_fn(move || -> Result<_, ()> {
        loop {
            match framed_stdin.poll().expect("Error while polling stdin") {
                Async::Ready(Some(line)) => {
                    println!("JGD. {}", line.to_string());
                   
                    let new_block = process_new_transaction(&line.to_string());
                    println!("Going to post mined block {:?}",new_block);
                    swarm.floodsub.publish(&floodsub_topic, new_block);

                    
                    
                    
                }
                Async::Ready(None) => panic!("Stdin closed"),
                Async::NotReady => break,
            };
        }

        loop {
            match swarm.poll().expect("Error while polling swarm") {
                Async::Ready(Some(_)) => {}
                Async::Ready(None) | Async::NotReady => {
                    if !listening {
                        if let Some(a) = Swarm::listeners(&swarm).next() {
                            println!("Listening on {:?}", a);
                            listening = true;
                        }
                    }
                    break;
                }
            }
        }

        Ok(Async::NotReady)
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_block() {
        //create blockchain
    let p2p_bc: Vec<Block> = vec![Block::genesis()];
    assert_eq!(p2p_bc[0].block_number , 1);
    assert_eq!(p2p_bc[0].transaction_list[0].transaction_details, "This is dummy transaction as genesis block has no transactions");
    }

    #[test]
    fn test_new_block() {
    let mut p2p_bc: Vec<Block> = vec![Block::genesis()];

    let new_txn = Transaction {
        transaction_id: String::from("1"),
        transaction_timestamp: 0,
        transaction_details: String::from("Testing a new transaction"),
    };
    let mut new_block = Block::new(vec![new_txn], &p2p_bc[p2p_bc.len() - 1]);

    Block::mine_new_block(&mut new_block, &PREFIX);
    p2p_bc.push(new_block);

    assert_eq!(p2p_bc.len(),2);
    assert_eq!(p2p_bc[1].transaction_list[0].transaction_details,"Testing a new transaction");
    }
}