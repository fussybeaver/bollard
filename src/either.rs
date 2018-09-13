use failure::Error;
use futures::Poll;
use futures::{future, Future, Stream};
use hyper::{Body, Response};

// https://github.com/rust-lang-nursery/futures-rs/pull/756
#[derive(Debug)]
pub(crate) enum EitherStream<A, B> {
    A(A),
    B(B),
}

impl<A, B> Stream for EitherStream<A, B>
where
    A: Stream,
    B: Stream<Item = A::Item, Error = A::Error>,
{
    type Item = A::Item;
    type Error = A::Error;

    fn poll(&mut self) -> Poll<Option<A::Item>, A::Error> {
        match *self {
            EitherStream::A(ref mut a) => a.poll(),
            EitherStream::B(ref mut b) => b.poll(),
        }
    }
}

pub(crate) enum EitherResponse<B, C, D> {
    A(future::FutureResult<Response<Body>, Error>),
    B(B),
    C(C),
    D(D),
}

impl<B, C, D> Future for EitherResponse<B, C, D>
where
    B: Future<Item = Response<Body>, Error = Error>,
    C: Future<Item = Response<Body>, Error = Error>,
    D: Future<Item = Response<Body>, Error = Error>,
{
    type Item = Response<Body>;
    type Error = Error;

    fn poll(&mut self) -> ::futures::Poll<Response<Body>, Error> {
        match *self {
            EitherResponse::A(ref mut a) => a.poll(),
            EitherResponse::B(ref mut b) => b.poll(),
            EitherResponse::C(ref mut c) => c.poll(),
            EitherResponse::D(ref mut d) => d.poll(),
        }
    }
}
