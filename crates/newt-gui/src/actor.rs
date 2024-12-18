use std::{sync::Arc, thread::JoinHandle};

use floem::{ext_event::create_signal_from_channel, reactive::ReadSignal};
use parking_lot::Mutex;

pub enum ActorMessage<SendToActor> {
    Shutdown,
    Custom(SendToActor),
}

pub struct ActorThread<SendToActor, RecvFromActor> {
    sender: crossbeam::channel::Sender<ActorMessage<SendToActor>>,
    receiver: crossbeam::channel::Receiver<RecvFromActor>,
    handle: Option<JoinHandle<()>>,
}

impl<SendToActor, RecvFromActor> Drop for ActorThread<SendToActor, RecvFromActor> {
    fn drop(&mut self) {
        self.sender.send(ActorMessage::Shutdown).unwrap();
        self.handle.take().unwrap().join().unwrap();
    }
}

impl<SendToActor: Send + 'static, RecvFromActor: Send + Clone + 'static>
    ActorThread<SendToActor, RecvFromActor>
{
    pub fn new<F: Fn(SendToActor) -> RecvFromActor + Send + 'static>(
        f: F,
    ) -> ActorThread<SendToActor, RecvFromActor> {
        let (send_to_director, recv_from_actor) = crossbeam::channel::unbounded();
        let (send_to_actor, recv_from_director) =
            crossbeam::channel::unbounded::<ActorMessage<SendToActor>>();

        let handle = std::thread::spawn(move || {
            let recv = recv_from_director;
            let send = send_to_director;

            loop {
                let val = recv.recv().expect("actor thread couldn't receive");
                match val {
                    ActorMessage::Shutdown => {
                        break;
                    }
                    ActorMessage::Custom(val) => {
                        let res = f(val);
                        send.send(res).expect("actor thread couldn't send");
                    }
                }
            }
        });
        Self {
            sender: send_to_actor,
            receiver: recv_from_actor,
            handle: Some(handle),
        }
    }

    pub fn send(
        &self,
        message: SendToActor,
    ) -> Result<(), crossbeam::channel::SendError<ActorMessage<SendToActor>>> {
        self.sender.send(ActorMessage::Custom(message))
    }

    /// receive a message from an actor, if there is one
    /// avaiable, otherwise None will be returned
    pub fn recv(&self) -> Option<RecvFromActor> {
        match self.receiver.try_recv() {
            Ok(v) => Some(v),
            Err(crossbeam::channel::TryRecvError::Empty) => None,
            _ => panic!("failed to receive message from actor"),
        }
    }

    pub fn recv_blocking(&self) -> RecvFromActor {
        self.receiver.recv().unwrap()
    }

    pub fn create_channel_from_receiver(&self) -> ReadSignal<Option<RecvFromActor>> {
        create_signal_from_channel(self.receiver.clone())
    }
}

#[test]
pub fn double_it() {
    let actor = ActorThread::new(|f: i32| f * 2);

    actor.send(3).unwrap();
    dbg!(actor.recv_blocking());
}
