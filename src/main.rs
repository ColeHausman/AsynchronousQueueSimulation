use message_payload::MessagePayload;
use mpi::ffi::QMPI_Mprobe;
use mpi::request::scope;
use mpi::request::{Request, RequestCollection, Scope};
use mpi::traits::*;
use mpi::Rank;
use mpi::Threading;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::message_payload::VectorClock;
use crate::process_data::ProcessData;
mod message_payload;
mod process_data;

fn handle_client(mut stream: TcpStream, tx: Sender<(i32, i32, MessagePayload)>, rank: i32) {
    let mut reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                println!("Client disconnected from process {}", rank);
                break;
            }
            Ok(_) => {
                println!("Process {} received: {}", rank, line.trim());
                // Attempt to parse the message
                if let Some(message_tuple) = parse_message(&line) {
                    tx.send(message_tuple)
                        .expect("Failed to send parsed message to MPI thread");
                } else {
                    println!("Failed to parse message at process {}: {}", rank, line);
                }
            }
            Err(e) => {
                println!("Failed to read from client at process {}: {}", rank, e);
                break;
            }
        }
    }
}

fn start_server(
    port: u16,
    tx: Sender<(i32, i32, MessagePayload)>,
    rank: i32,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    println!("Process {} server listening on port {}", rank, port);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let tx = tx.clone();
                thread::spawn(move || {
                    handle_client(stream, tx, rank);
                });
            }
            Err(e) => {
                println!("Failed to accept client at process {}: {}", rank, e);
            }
        }
    }
    Ok(())
}

fn parse_message(input: &str) -> Option<(i32, i32, MessagePayload)> {
    let mut process = None;
    let mut op = None;
    let mut val = None;

    // Split and parse the input
    for part in input.split(',') {
        let mut iter = part.trim().split(':');
        match (iter.next(), iter.next()) {
            (Some("process"), Some(value)) => process = value.trim().parse::<i32>().ok(),
            (Some("op"), Some(value)) => op = value.trim().parse::<i32>().ok(),
            (Some("value"), Some(value)) => val = value.trim().parse::<i32>().ok(),
            _ => {}
        }
    }

    if let (Some(process), Some(op), Some(val)) = (process, op, val) {
        Some((
            process,
            process,
            MessagePayload::new(op, val, process, process, VectorClock::default()),
        ))
    } else {
        None
    }
}

fn main() {
    let universe = mpi::initialize().unwrap();

    let world = universe.world();
    let size = world.size();
    let rank = world.rank();

    let base_port = 7878; // Base port number
    let port = base_port + rank as u16; // Unique port for each process

    let (tx, rx): (
        Sender<(i32, i32, MessagePayload)>,
        Receiver<(i32, i32, MessagePayload)>,
    ) = mpsc::channel();

    // Start the server in a separate thread for each MPI process
    let tx_clone = tx.clone();
    thread::spawn(move || {
        start_server(port, tx_clone, rank).unwrap();
    });

    let mut process_data = ProcessData::new(rank, world.size());
    let mut data_buffer = vec![MessagePayload::default(); 100];
    let mut gen_buffer = Vec::new();
    let mut data_buffer_iter = data_buffer.iter_mut();

    let mut msgs: VecDeque<(i32, i32, MessagePayload)> = VecDeque::new();
    msgs.push_back((
        0,
        0,
        MessagePayload::new(0, 69, 0, 0, VectorClock::new(size)),
    ));

    msgs.push_back((
        1,
        1,
        MessagePayload::new(0, 420, 1, 1, VectorClock::new(size)),
    ));

    loop {
        // Process non-blocking receives
        if let Some(recv_buf) = data_buffer_iter.next() {
            // Initiate non-blocking receives within a scope
            mpi::request::multiple_scope(1, |scope, coll| {
                let request = world.any_process().immediate_receive_into(scope, recv_buf);
                coll.add(request);

                loop {
                    // Non-blocking check for completion from `coll`
                    match coll.test_any() {
                        Some((_, status, result)) => {
                            // Handle the completion here
                            println!(
                                "Process {} received {:?} from process {}",
                                rank,
                                result,
                                status.source_rank(),
                            );
                            gen_buffer.push(result);
                            process_data.message_buffer[0] = process_data.execute_locally(*result);
                            break;
                        }
                        _ => {
                            while let Ok(data) = rx.try_recv() {
                                // Echo or process the message further, here we just send to the next process in a simple ring
                                msgs.push_back((data.0, data.1, data.2));
                                println!("{:?}", msgs);
                            }
                            while !msgs.is_empty() {
                                let message = msgs.pop_front().unwrap();
                                if message.0 == rank {
                                    world.process_at_rank(message.1).send(&message.2);
                                }
                            }
                        }
                    }
                }
            });
        }

        // Generate new messages
        for msg in process_data.generate_messages() {
            msgs.push_back(msg);
        }
    }
}
