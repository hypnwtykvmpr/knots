use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

pub(crate) fn command_for_program(program: impl AsRef<OsStr>) -> Command {
    let program = program.as_ref();
    #[cfg(windows)]
    {
        if Path::new(program)
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
        {
            let mut command = Command::new("powershell.exe");
            command
                .arg("-NoProfile")
                .arg("-ExecutionPolicy")
                .arg("Bypass")
                .arg("-File")
                .arg(program);
            return command;
        }
    }
    Command::new(program)
}
