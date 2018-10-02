pub mod counter {
    use std::thread;
    use std::sync::mpsc::{sync_channel, channel, Sender, SyncSender};

    enum Command {
        Get,
        Set(u32),
        Stop
    }

    #[derive(Debug, PartialEq)]
    pub enum Answer {
        Count(u32),
        Nothing
    }

    pub struct CounterState {
        count: u32
    }

    pub struct Counter {
        sender: Sender<(SyncSender<Answer>, Command)>,
        _handle: thread::JoinHandle<()>,
    }

    pub fn start(count: u32) -> Counter {
        let (sender, receiver) = channel();
        let handle = thread::spawn(move || {
            println!("Starting counter service...");
            let mut state = CounterState { count: count };

            loop {
                let cmd: (SyncSender<Answer>, Command) = receiver.recv().unwrap();

                match cmd {
                    (ch, Command::Get) => {
                        ch.send(Answer::Count(state.count)).unwrap();
                    },
                    (ch, Command::Set(count)) => {
                        let old_count = state.count;
                        state.count = count;
                        ch.send(Answer::Count(old_count)).unwrap();
                    },
                    (ch, Command::Stop) => {
                        ch.send(Answer::Nothing).unwrap();
                        break;
                    }
                }
            }
            println!("Stopping counter service...");
        });

        Counter {
            sender: sender,
            _handle: handle
        }
    }

    fn query(counter: &Counter, cmd: Command) -> Answer {
        let (sender, receiver) = sync_channel(0);
        counter.sender.send((sender, cmd)).unwrap();

        receiver.recv().unwrap()
    }

    pub fn get(counter: &Counter) -> Answer {
        query(counter, Command::Get)
    }

    pub fn set(counter: &Counter, count: u32) -> Answer {
        query(counter, Command::Set(count))
    }

    pub fn stop(counter: &Counter) {
        query(counter, Command::Stop);
    }
}
