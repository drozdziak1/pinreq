use failure::Error;

use std::{collections::HashMap, fs::File, io::Read};

use crate::{matrix::MatrixChannelSettings, req_channel::ChannelSettings};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub matrix: Vec<MatrixChannelSettings>,
}

impl Config {
    pub fn from_file(name: &str) -> Result<Self, Error> {
        let mut file = File::open(name)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        Ok(toml::from_str(contents.as_str())?)
    }
    /// Convert the config to a channel name -> settings map
    pub fn to_map(self) -> Result<HashMap<String, Box<impl ChannelSettings>>, Error> {
        let mut ret = HashMap::new();

        for matrix_ch in self.matrix {
            // Verify global channel name uniqueness
            if ret.contains_key(&matrix_ch.name) {
                bail!(
                    "Ambiguous channel name {}, please rename conflicted channels",
                    matrix_ch.name
                );
            }

            ret.insert(matrix_ch.name.clone(), Box::new(matrix_ch));
        }

        Ok(ret)
    }
}
