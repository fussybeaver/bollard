use mio_named_pipes::NamedPipe;

pub struct NamedPipeStream {
    io: PollEvented<NamedPipe>,
}

#[derive(Debug)]
pub struct ConnectFuture {
    inner: State,
}

#[derive(Debug)]
enum State {
    Waiting(NamedPipe),
    PendingConnection(NamedPipe),
    Error(io::Error),
    Empty,
}

impl NamedPipeStream {
    pub fn connect<P>(addr: A) -> ConnectFuture
    where
        A: AsRef<OsStr>,
    {
        let stream = mio_named_pipes::NamedPipe::new(addr).map(NamedPipeStream::new);

        let inner = match NamedPipe::connect(stream) {
            Ok(_) => State::Waiting(stream),
            Err(io::ErrorKind::WouldBlock) => State::PendingConnection(stream),
            Err(e) => State::Error(e),
        };

        ConnectFuture { inner }
    }

    pub fn new(stream: mio_named_pipes::NamedPipe) -> NamedPipeStream {
        let io = PollEvented::new(stream);
        NamedPipeStream { io }
    }
}

impl Future for ConnectFuture {
    type Item = NamedPipe;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<NamedPipe, io::Error> {
        unimplemented!();
    }
}
