#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, argh::FromArgs)]
/// ActiveSG Slot Dataminer
pub struct Args {
    /// username
    #[argh(option, short = 'u')]
    pub username: String,

    /// users password
    #[argh(option, short = 'p')]
    pub password: String,

    /// output data in struct of array
    #[argh(switch, short = 's')]
    pub is_soa: bool,
}
