use failure::Error;

use std::str::FromStr;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Message {
    /// "Please pin this for me"
    Pin(String),
    /// "I have pinned this"
    Confirm(String),
}

impl FromStr for Message {
    type Err = Error;
    fn from_str(s: &str) -> Result<Message, Self::Err> {
        let mut cmd_iter = s.splitn(2, ' ');

        let cmd = match cmd_iter.next() {
            Some(c) => c,
            None => bail!("Could not extract command name"),
        };
        let args = match cmd_iter.next() {
            Some(a) => a,
            None => bail!("Could not extract command arguments"),
        };

        match (cmd, args) {
            ("pin", arg) => Ok(Message::Pin(arg.to_owned())),
            ("confirm", arg) => Ok(Message::Confirm(arg.to_owned())),
            (other_cmd, other_args) => Err(format_err!(
                "Could not understand command {:?} with args {:?}",
                other_cmd,
                other_args
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinreqmsg_from_str_test() {
        assert_eq!(
            "pin something".parse::<Message>().unwrap(),
            Message::Pin("something".to_owned())
        );

        assert_eq!(
            "confirm something".parse::<Message>().unwrap(),
            Message::Confirm("something".to_owned())
        );
    }
}
