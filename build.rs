use anyhow::Result;
use vergen::EmitBuilder;

fn main() -> Result<()> {
    EmitBuilder::builder()
        .git_sha(true)
        .git_commit_message()
        .fail_on_error()
        .emit()?;
    Ok(())
}
