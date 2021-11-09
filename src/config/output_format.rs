use clap::arg_enum;
// 
arg_enum! {
    #[derive(Copy, Clone, Debug, PartialEq)]
    pub enum OutputFormat {
        Csv,
        Json,
        Text,
    }
}

/// implement a default for OutputFormat
impl Default for OutputFormat {
    /// return default (In this case its a str)
    fn default() -> Self {
        Self::Text
    }
}
