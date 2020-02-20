#![deny(rust_2018_idioms, unused, unused_import_braces, unused_qualifications, warnings)]

use {
    std::{
        cmp::Ordering::*,
        env,
        fs::File,
        io::prelude::*,
        path::Path,
        process::Command,
        time::Duration
    },
    dir_lock::DirLock,
    msegen::{
        github::Repo,
        util::{
            CommandOutputExt as _,
            Error,
            IoResultExt as _
        },
        version::version
    }
};

fn release_client() -> Result<reqwest::blocking::Client, Error> { //TODO return an async client instead
    let mut headers = reqwest::header::HeaderMap::new();
    let mut token = String::default();
    File::open("assets/release-token").at("assets/release-token")?.read_to_string(&mut token).at("assets/release-token")?;
    headers.insert(reqwest::header::AUTHORIZATION, reqwest::header::HeaderValue::from_str(&format!("token {}", token))?);
    headers.insert(reqwest::header::USER_AGENT, reqwest::header::HeaderValue::from_static(concat!("magic-set-generator/", env!("CARGO_PKG_VERSION"))));
    Ok(reqwest::blocking::Client::builder().default_headers(headers).timeout(Duration::from_secs(600)).build()?)
}

fn main() -> Result<(), Error> {
    let client = release_client()?;
    //TODO make sure working dir is clean and on master and up to date with remote and remote is up to date
    let repo = Repo::new("fenhl", "magic-set-generator");
    if let Some(latest_release) = repo.latest_release(&client)? {
        let remote_version = latest_release.version()?;
        match version().cmp(&remote_version) {
            Less => { return Err(Error::VersionRegression); }
            Equal => { return Err(Error::SameVersion); }
            Greater => {}
        }
    }
    let lock_dir = Path::new(&env::var_os("TEMP").ok_or(Error::MissingEnvar("TEMP"))?).join("syncbin-startup-rust.lock");
    let lock = DirLock::new_sync(&lock_dir);
    Command::new("rustup").arg("update").arg("stable").check("rustup")?;
    Command::new("rustup").arg("update").arg("stable-u686-pc-windows-msvc").check("rustup")?;
    drop(lock);
    Command::new("cargo").arg("build").arg("--bin=msg-gui").arg("--release").check("cargo")?;
    Command::new("cargo").arg("+stable-i686-pc-windows-msvc").arg("build").arg("--bin=msg-gui").arg("--release").arg("--target-dir=target-x86").check("cargo")?;
    let release_notes = {
        let mut release_notes_file = tempfile::Builder::new()
            .prefix("msg-release-notes")
            .suffix(".md")
            .tempfile().at_unknown()?;
        Command::new("nano").arg(release_notes_file.path()).check("nano")?;
        let mut buf = String::default();
        release_notes_file.read_to_string(&mut buf).at(release_notes_file.path())?;
        buf
    };
    let release = repo.create_release(&client, format!("Magic Set Generator {}", version()), format!("v{}", version()), release_notes)?;
    repo.release_attach(&client, &release, "msg-win64.exe", "application/vnd.microsoft.portable-executable", File::open("target/release/msg-gui.exe").at("target/release/msg-gui.exe")?)?;
    repo.release_attach(&client, &release, "msg-win32.exe", "application/vnd.microsoft.portable-executable", File::open("target-x86/release/msg-gui.exe").at("target-x86/release/msg-gui.exe")?)?;
    repo.publish_release(&client, release)?;
    Ok(())
}
