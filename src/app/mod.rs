// app - application logic and the four Zephyr threads
//
// Health/Comms/Button/LED own the threads; ipc carries the shared state and
// the fixed-size queues; fault and identity are the application services.

pub mod button;
pub mod comms;
pub mod fault;
pub mod health;
pub mod identity;
pub mod ipc;
pub mod led;
