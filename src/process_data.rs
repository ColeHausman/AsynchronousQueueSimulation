use mpi::request::{RequestCollection, Scope};
use mpi::traits::*;
use mpi::Rank;
use std::cmp::max;
use std::collections::VecDeque;

use crate::message_payload::{MessagePayload, VectorClock};

pub struct ProcessData {
    rank: Rank,
    world_size: i32,
    pub timestamp: VectorClock,
    pub enq_count: i32,
    pub pending_dequeues: Vec<ConfirmationList>,
    pub message_buffer: Vec<MessagePayload>,
    pub message_history: Vec<MessagePayload>,
    pub local_queue: VecDeque<(i32, Rank, VectorClock)>,
}

impl ProcessData {
    pub fn new(rank: Rank, size: i32) -> Self {
        ProcessData {
            rank,
            world_size: size,
            timestamp: VectorClock::new(size),
            enq_count: 0,
            pending_dequeues: Vec::new(),
            message_buffer: vec![MessagePayload::with_msg(-1); size as usize],
            message_history: Vec::new(),
            local_queue: VecDeque::new(),
        }
    }

    pub fn increment_ts(&mut self) {
        self.timestamp.clock[self.rank as usize] += 1;
    }

    pub fn update_ts(&mut self, v_j: &VectorClock) {
        let max_index = self.timestamp.size;
        for i in 0..max_index {
            if v_j.clock[i] > self.timestamp.clock[i] {
                self.timestamp.clock[i] = v_j.clock[i];
            }
        }
    }

    fn find_insert_position(&self, value: &VectorClock) -> usize {
        self.local_queue
            .iter()
            .position(|x| value < &x.2)
            .unwrap_or(self.local_queue.len())
    }

    pub fn ordered_insert(&mut self, value: (i32, Rank, VectorClock)) {
        let position = self.find_insert_position(&value.2);
        self.local_queue.insert(position, value);
    }

    pub fn propagate_earlier_responses(&mut self) {
        for row in (1..self.pending_dequeues.len()).rev() {
            for col in 0..self.pending_dequeues[0].response_buffer.len() {
                let response = self.pending_dequeues[row].response_buffer[col];
                if response != 0 && self.pending_dequeues[row - 1].response_buffer[col] == 0 {
                    self.pending_dequeues[row - 1].response_buffer[col] = response;
                }
            }
        }
    }

    pub fn update_unsafes(&mut self, start_index: usize) {
        let invoker = self.pending_dequeues[start_index].invoker;
        for i in start_index..self.pending_dequeues.len() {
            self.pending_dequeues[i].response_buffer[invoker as usize] = 1;
        }
    }

    pub fn execute_locally(&mut self, message_payload: MessagePayload) -> MessagePayload {
        match message_payload.message {
            0 => {
                // EnqInvoke
                self.enq_count = 0;
                self.increment_ts();
                MessagePayload::new(
                    1,
                    message_payload.value,
                    self.rank,
                    self.rank,
                    self.timestamp,
                )
            }
            1 => {
                // Receive EnqRequest
                self.update_ts(&message_payload.time_stamp);
                self.ordered_insert((
                    message_payload.value,
                    message_payload.invoker,
                    message_payload.time_stamp,
                ));
                for confirmation_list in self.pending_dequeues.iter_mut() {
                    if confirmation_list.ts < message_payload.time_stamp {
                        confirmation_list.response_buffer[message_payload.invoker as usize] = 1;
                    }
                }
                MessagePayload::new(
                    2,
                    message_payload.value,
                    message_payload.invoker,
                    self.rank,
                    self.timestamp,
                )
            }
            2 => {
                // Enq ack
                self.enq_count += 1;
                if self.enq_count == self.world_size {
                    println!("Process{} done enqueueing!", self.rank); // TODO make actual return
                                                                       // system
                }
                MessagePayload::new(-1, 0, 0, 0, VectorClock::default())
            }
            _ => MessagePayload::default(),
        }
    }

    pub fn generate_messages(&self) -> Vec<(i32, i32, MessagePayload)> {
        let mut gen_messages: Vec<(i32, i32, MessagePayload)> = Vec::new();
        let invoker = self.message_buffer[0].invoker;
        let message = self.message_buffer[0].message;
        match message {
            1 => {
                let message_payload = self.message_buffer[0];
                for recv_rank in 0..self.world_size {
                    gen_messages.push((self.rank, recv_rank, message_payload));
                }
            }
            2 => {
                let message_payload = self.message_buffer[0];

                gen_messages.push((self.rank, invoker, message_payload));
            }
            _ => {}
        }
        gen_messages
    }
}

#[derive(Clone, Default, Debug)]
pub struct ConfirmationList {
    pub response_buffer: Vec<i32>,
    pub ts: VectorClock,
    pub invoker: Rank,
    pub handled: bool,
}

impl ConfirmationList {
    pub fn new(size: i32, deq_ts: VectorClock, deq_invoker: Rank) -> Self {
        ConfirmationList {
            response_buffer: vec![0; size as usize],
            ts: deq_ts,
            invoker: deq_invoker,
            handled: false,
        }
    }
}
