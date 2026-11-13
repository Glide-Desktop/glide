pub fn spawn_detached<I, S>(program: &str, args: I) -> Result<(), std::io::Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = std::process::Command::new(program);
    command.args(args);
    crate::spawn_detached(command).map(|_| ()).map_err(|err| std::io::Error::other(err.to_string()))
}
