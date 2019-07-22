use failure::Error;

use std::{fs::File, io::Read};

use crate::matrix::MatrixChannelSettings;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub matrix: Vec<MatrixChannelSettings>
}

impl Config {
    pub fn from_file(name: &str) -> Result<Self, Error> {
        let mut file = File::open(name)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        Ok(toml::from_str(contents.as_str())?)
    }
}
