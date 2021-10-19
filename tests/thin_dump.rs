use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;

mod common;

use common::common_args::*;
use common::fixture::*;
use common::input_arg::*;
use common::process::*;
use common::program::*;
use common::target::*;
use common::test_dir::*;
use common::thin::*;

//------------------------------------------

const USAGE: &str =
"thin_dump 0.9.0
Dump thin-provisioning metadata to stdout in XML format

USAGE:
    thin_dump [FLAGS] [OPTIONS] <INPUT>

FLAGS:
    -q, --quiet            Suppress output messages, return only exit code.
    -r, --repair           Repair the metadata whilst dumping it
        --skip-mappings    Do not dump the mappings
    -h, --help             Prints help information
    -V, --version          Prints version information

OPTIONS:
        --data-block-size <SECTORS>                Provide the data block size for repairing
    -m, --metadata-snapshot <METADATA_SNAPSHOT>    Access the metadata snapshot on a live pool
        --nr-data-blocks <NUM>                     Override the number of data blocks if needed
    -o, --output <FILE>                            Specify the output file rather than stdout
        --transaction-id <NUM>                     Override the transaction id if needed

ARGS:
    <INPUT>    Specify the input device to dump";

//-----------------------------------------

struct ThinDump;

impl<'a> Program<'a> for ThinDump {
    fn name() -> &'a str {
        "thin_dump"
    }

    fn cmd<I>(args: I) -> Command
    where
        I: IntoIterator,
        I::Item: Into<std::ffi::OsString>,
    {
        thin_dump_cmd(args)
    }

    fn usage() -> &'a str {
        USAGE
    }

    fn arg_type() -> ArgType {
        ArgType::InputArg
    }

    fn bad_option_hint(option: &str) -> String {
        msg::bad_option_hint(option)
    }
}

impl<'a> InputProgram<'a> for ThinDump {
    fn mk_valid_input(td: &mut TestDir) -> Result<std::path::PathBuf> {
        mk_valid_md(td)
    }

    fn file_not_found() -> &'a str {
        msg::FILE_NOT_FOUND
    }

    fn missing_input_arg() -> &'a str {
        msg::MISSING_INPUT_ARG
    }

    fn corrupted_input() -> &'a str {
        msg::BAD_SUPERBLOCK
    }
}

//------------------------------------------

test_accepts_help!(ThinDump);
test_accepts_version!(ThinDump);
test_rejects_bad_option!(ThinDump);

test_missing_input_arg!(ThinDump);
test_input_file_not_found!(ThinDump);
test_input_cannot_be_a_directory!(ThinDump);
test_unreadable_input_file!(ThinDump);

//------------------------------------------
// test dump & restore cycle

#[test]
fn dump_restore_cycle() -> Result<()> {
    let mut td = TestDir::new()?;

    let md = mk_valid_md(&mut td)?;
    let output = run_ok_raw(thin_dump_cmd(args![&md]))?;

    let xml = td.mk_path("meta.xml");
    let mut file = OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .open(&xml)?;
    file.write_all(&output.stdout[0..])?;
    drop(file);

    let md2 = mk_zeroed_md(&mut td)?;
    run_ok(thin_restore_cmd(args!["-i", &xml, "-o", &md2]))?;

    let output2 = run_ok_raw(thin_dump_cmd(args![&md2]))?;
    assert_eq!(output.stdout, output2.stdout);

    Ok(())
}

//------------------------------------------
// test no stderr with a normal dump

#[test]
#[cfg(not(feature = "rust_tests"))]
fn no_stderr() -> Result<()> {
    let mut td = TestDir::new()?;

    let md = mk_valid_md(&mut td)?;
    let output = run_ok_raw(thin_dump_cmd(args![&md]))?;

    assert_eq!(output.stderr.len(), 0);
    Ok(())
}

//------------------------------------------
// test superblock overriding & repair
// TODO: share with thin_repair

#[cfg(not(feature = "rust_tests"))]
fn override_something(flag: &str, value: &str, pattern: &str) -> Result<()> {
    let mut td = TestDir::new()?;
    let md = mk_valid_md(&mut td)?;
    let output = run_ok_raw(thin_dump_cmd(args![&md, flag, value]))?;

    if !cfg!(feature = "rust_tests") {
        assert_eq!(output.stderr.len(), 0);
    }
    assert!(std::str::from_utf8(&output.stdout[0..])?.contains(pattern));
    Ok(())
}

#[test]
#[cfg(not(feature = "rust_tests"))]
fn override_transaction_id() -> Result<()> {
    override_something("--transaction-id", "2345", "transaction=\"2345\"")
}

#[test]
#[cfg(not(feature = "rust_tests"))]
fn override_data_block_size() -> Result<()> {
    override_something("--data-block-size", "8192", "data_block_size=\"8192\"")
}

#[test]
#[cfg(not(feature = "rust_tests"))]
fn override_nr_data_blocks() -> Result<()> {
    override_something("--nr-data-blocks", "234500", "nr_data_blocks=\"234500\"")
}

// FIXME: duplicate with superblock_succeeds in thin_repair.rs
#[test]
fn repair_superblock() -> Result<()> {
    let mut td = TestDir::new()?;
    let md = mk_valid_md(&mut td)?;
    let before = run_ok_raw(thin_dump_cmd(args![&md]))?;
    damage_superblock(&md)?;

    let after = run_ok_raw(thin_dump_cmd(
        args![
            "--repair",
            "--transaction-id=1",
            "--data-block-size=128",
            "--nr-data-blocks=20480",
            &md
        ],
    ))?;
    if !cfg!(feature = "rust_tests") {
        assert_eq!(after.stderr.len(), 0);
    }
    assert_eq!(before.stdout, after.stdout);

    Ok(())
}

//------------------------------------------
// test compatibility between options
// TODO: share with thin_repair

#[test]
#[cfg(not(feature = "rust_tests"))]
fn missing_transaction_id() -> Result<()> {
    let mut td = TestDir::new()?;
    let md = mk_valid_md(&mut td)?;
    damage_superblock(&md)?;
    let stderr = run_fail(
        thin_dump_cmd(
        args![
            "--repair",
            "--data-block-size=128",
            "--nr-data-blocks=20480",
            &md
        ],
    ))?;
    assert!(stderr.contains("transaction id"));
    Ok(())
}

#[test]
fn missing_data_block_size() -> Result<()> {
    let mut td = TestDir::new()?;
    let md = mk_valid_md(&mut td)?;
    damage_superblock(&md)?;
    let stderr = run_fail(
        thin_dump_cmd(
        args![
            "--repair",
            "--transaction-id=1",
            "--nr-data-blocks=20480",
            &md
        ],
    ))?;
    assert!(stderr.contains("data block size"));
    Ok(())
}

#[test]
#[cfg(not(feature = "rust_tests"))]
fn missing_nr_data_blocks() -> Result<()> {
    let mut td = TestDir::new()?;
    let md = mk_valid_md(&mut td)?;
    damage_superblock(&md)?;
    let stderr = run_fail(
        thin_dump_cmd(
        args![
            "--repair",
            "--transaction-id=1",
            "--data-block-size=128",
            &md
        ],
    ))?;
    assert!(stderr.contains("nr data blocks"));
    Ok(())
}

//------------------------------------------
