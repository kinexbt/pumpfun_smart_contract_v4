use anyhow::Error;
#[derive(Debug)]
#[allow(dead_code)]
pub struct AppError(Error);

impl<E> From<E> for AppError
where
    E: Into<Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}