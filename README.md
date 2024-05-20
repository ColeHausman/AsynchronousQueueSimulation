# Simulating an Asynchronous Queue using MPI
Based on the work by Samuel Baldwin, Cole Hausman, Mohamed Bakr, and Edward Talmage

## Initializing the simulation
1. Within the repo build using `cargo build`
2. Then execute using mpi:
```
mpiexec -n <Num Processes> ./target/debug/async_queue
```
## Sending Queue Operations to Processes
The simulator allows for external requests through the use of TCP connections.\
Using whatever tool you prefer message requests can easily be sent to the process of your choice.\
Here I use `netcat`:
```
echo "process: <Process Number>, op: <Operation Number to Execute>, value: <Value to enqueue (if enqueuing)>" | nc 127.0.0.1 <PORT> 
```
The base PORT is 8000 and increments by 1 for each process 0 to n, for example to have process 2 Enqueue the value 12 you would do:
```
echo "process: 2, op: 0, value: 12" | nc 127.0.0.1 8002 
```
If you wanted to have process 1 Dequeue you would do:
```
echo "process: 1, op: 3, value: 0" | nc 127.0.0.1 8001
```
*Note*: The value of a dequeue does not matter and will be ignored by the system\
\
If you want to have process 0 and process 1 Enqueue at the same time you would do:
```
echo "process: 0, op: 0, value: 12" | nc 127.0.0.1 8000 && 
echo "process: 1, op: 0, value: 13" | nc 127.0.0.1 8001 
```
If you need to execute high volumes of enqueues and dequeues its recommended to create a bash script to execute them in rapid succession.\
Only Enqueue invokes (0) and Dequeue invokes (3) are allowed, any other message type will break the system\
*Note*: This algorithm is designed for true asynchrony so order of message arrivals is not guranteed 

## Memory
**MPI Buffers** \
In order to simulate the asynchronous nature of this algorithm, any message must be allowed to arrive at any time from the invocation to the termination of the program. As such, a unique mutable buffer is required for each message sent, an enqueue will require `2n + 1` messages while a dequeue will require `n^2 + n + 1` messages where `n` is the number of processes. The number of mutable buffers grows dynamically which should be kept in mind when increasing the number of processes.\
\
**Vector Clock Implementation** \
Vector Clocks are represented as an array of i32s which would ideally have a length equal to the number of processes in the system. However, MPI needs to know the length of the array at compile time which means we would have to change the constant array size and recompile each time we want to change the number of processes. In order to mitigate this, VectorClocks are compiled with an upper bound on their length and we use only the first **size** elements where **size** is the number of processes in the system. In order to set your own maximum number of processes the `MAX_BUFFER_SIZE` constant must be set in `message_payload.rs`
