#[derive(Clone, Debug, Fail)]
pub enum MatrixError {
    #[fail(display = "You need an active auth token to use a Matrix channel")]
    NotAuthenticated,
    #[fail(display = "Matrix' response could not be parsed: {}", _0)]
    ResponseNotUnderstood(String),
    #[fail(display = "Room {} is not joined", _0)]
    RoomNotJoined(String),
}
