use super::*;

macro_rules! unwrap_as {
    ($e:expr, $p:path) => {
        match $e {
            $p(s) => s,
            _ => panic!("Not a {}: {:?}", stringify!($p), $e),
        }
    }
}

macro_rules! assert_extent {
    ($parsed:expr, $extent:expr) => (assert_eq!($parsed.extent(), $extent));
}

type TestResult<T> = Result<(usize, T), (usize, Vec<Error>)>;

pub fn parse_full<'s, F, T>(f: F, s: &'s str) -> TestResult<T>
    where F: for<'a> FnOnce(&mut Master<'a>, Point<'a>) -> Progress<'a, T>
{
    // TODO: Master::once()?
    let tokens = Tokens::new(s).collect::<Result<Vec<_>, _>>().expect("Unable to tokenize");
    let (_ws, tokens): (Vec<_>, Vec<_>) = tokens.into_iter().partition(|t| {
        t.is_whitespace() || t.is_comment() || t.is_doc_comment() || t.is_comment_block() || t.is_doc_comment_block()
    });

    let mut pm = Master::with_state(State::new());
    let pt = Point::new(&tokens);
    let r = f(&mut pm, pt);
    match pm.finish(r) {
        peresil::Progress { status: peresil::Status::Success(v), point } => {
            Ok((point.offset, v))
        }
        peresil::Progress { status: peresil::Status::Failure(v), point } => {
            Err((point.offset, v))
        }
    }
}

pub fn qp<'s, F, T>(f: F, s: &'s str) -> T
    where F: for<'a> FnOnce(&mut Master<'a>, Point<'a>) -> Progress<'a, T>
{
    unwrap_progress(parse_full(f, s))
}

pub fn unwrap_progress<T>(p: TestResult<T>) -> T {
    match p {
        Ok((_, v)) => v,
        Err((offset, e)) => panic!("Failed parsing at token at index {}: {:?}", offset, e),
    }
}

pub fn unwrap_progress_err<T>(p: TestResult<T>) -> (usize, Vec<Error>) {
    match p {
        Ok(_) => panic!("Parsing should have failed, but it did not"),
        Err(x) => x
    }
}
