use failure::*;
use std::env::var;
use std::fs;
use std::io::*;
use std::path::*;
use std::process::Command;
use zip::ZipArchive;

fn download_archive_windows(out_dir: &Path) -> Fallible<()> {
    if out_dir.join("libfftw3.dll").exists() && out_dir.join("libfftw3f.dll").exists() {
        return Ok(());
    }

    let archive = out_dir.join("fftw_windows.zip");
    if !archive.exists() {
        // Download
        let mut conn = ftp::FtpStream::connect("ftp.fftw.org:21")?;
        conn.login("anonymous", "anonymous")?;
        conn.cwd("pub/fftw")?;
        let buf = conn.simple_retr("fftw-3.3.5-dll64.zip")?.into_inner();
        // TODO calc checksum
        let mut f = fs::File::create(&archive)?;
        f.write(&buf)?;
    }
    let f = fs::File::open(&archive)?;
    let mut zip = ZipArchive::new(f)?;
    let target = var("TARGET").unwrap();
    for name in &["fftw3-3", "fftw3f-3"] {
        for ext in &["dll", "def"] {
            let filename = format!("lib{}.{}", name, ext);
            let mut zf = zip.by_name(&filename)?;
            let mut f = fs::File::create(out_dir.join(filename))?;
            copy(&mut zf, &mut f)?;
        }
        run(cc::windows_registry::find_tool(&target, "lib.exe")
            .unwrap()
            .to_command()
            .arg("/MACHINE:X64")
            .arg(format!("/DEF:lib{}.def", name))
            .arg(format!("/OUT:lib{}.lib", name))
            .current_dir(out_dir))
    }
    Ok(())
}

fn download_archive_unix(out_dir: &Path) -> Fallible<()> {
    const FFTW: &'static str = "fftw-3.3.6-pl1";
    const MD5SUM: &'static str = "682a0e78d6966ca37c7446d4ab4cc2a1";

    if out_dir.join("lib/libfftw3.a").exists() && out_dir.join("lib/libfftw3f.a").exists() {
        return Ok(());
    }

    // Download
    let uri = format!("http://www.fftw.org/{}.tar.gz", FFTW);
    let mut res = reqwest::get(&uri)?;
    if !res.status().is_success() {
        bail!(
            "HTTP access to {} is failed with status = {}",
            uri,
            res.status()
        );
    }
    let mut buf = Vec::new();
    res.copy_to(&mut buf)?;

    // Verify downloaded archive by md5sum
    let md5_sum = format!("{:x}", md5::compute(&buf));
    if md5_sum != MD5SUM {
        bail!(
            "md5sum of downloaded archive is different: actual={}, correct={}",
            md5_sum,
            MD5SUM
        );
    }

    // Write down to archive
    let archive_file = out_dir.join(format!("{}.tar.gz", FFTW));
    let mut f = BufWriter::new(fs::File::create(&archive_file)?);
    f.write(&buf)?;

    // Expand
    let st = Command::new("tar")
        .arg("xf")
        .arg(&archive_file)
        .current_dir(&out_dir)
        .status()?;
    if !st.success() {
        bail!("Failed to expand archive");
    }

    // Build FFTW
    let archive_dir = out_dir.join(FFTW);
    build_fftw(&["--enable-single"], &archive_dir, &out_dir);
    build_fftw(&[], &archive_dir, &out_dir);
    Ok(())
}

fn build_fftw(flags: &[&str], src_dir: &Path, out_dir: &Path) {
    run(Command::new("./configure")
        .arg("--with-pic")
        .arg("--enable-static")
        .arg(format!("--prefix={}", out_dir.display()))
        .args(flags)
        .current_dir(&src_dir));
    run(Command::new("make")
        .arg(format!("-j{}", var("NUM_JOBS").unwrap()))
        .current_dir(&src_dir));
    run(Command::new("make").arg("install").current_dir(&src_dir));
}

fn run(command: &mut Command) {
    println!("Running: {:?}", command);
    match command.status() {
        Ok(status) => {
            if !status.success() {
                panic!("`{:?}` failed: {}", command, status);
            }
        }
        Err(error) => {
            panic!("failed to execute `{:?}`: {}", command, error);
        }
    }
}

fn main() -> Fallible<()> {
    let out_dir = PathBuf::from(var("OUT_DIR").unwrap());
    if cfg!(target_os = "windows") {
        download_archive_windows(&out_dir)?;
        println!("cargo:rustc-link-search={}", out_dir.display());
        println!("cargo:rustc-link-lib=libfftw3-3");
        println!("cargo:rustc-link-lib=libfftw3f-3");
    } else {
        download_archive_unix(&out_dir)?;
        println!("cargo:rustc-link-search={}", out_dir.join("lib").display());
        println!("cargo:rustc-link-lib=static=fftw3");
        println!("cargo:rustc-link-lib=static=fftw3f");
    }
    Ok(())
}
