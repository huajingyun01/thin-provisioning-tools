use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::io_engine::{AsyncIoEngine, IoEngine, SyncIoEngine};
use crate::pdata::space_map_metadata::core_metadata_sm;
use crate::report::mk_quiet_report;
use crate::thin::ir::MetadataVisitor;
use crate::thin::restore::Restorer;
use crate::write_batcher::WriteBatcher;

//------------------------------------------

const MAX_CONCURRENT_IO: u32 = 1024;

//------------------------------------------

pub trait MetadataGenerator {
    fn generate_metadata(&self, v: &mut dyn MetadataVisitor) -> Result<()>;
}

struct ThinGenerator;

impl MetadataGenerator for ThinGenerator {
    fn generate_metadata(&self, _v: &mut dyn MetadataVisitor) -> Result<()> {
        Ok(()) // TODO
    }
}

//------------------------------------------

fn format(engine: Arc<dyn IoEngine + Send + Sync>, gen: ThinGenerator) -> Result<()> {
    let sm = core_metadata_sm(engine.get_nr_blocks(), u32::MAX);
    let batch_size = engine.get_batch_size();
    let mut w = WriteBatcher::new(engine, sm, batch_size);
    let mut restorer = Restorer::new(&mut w, Arc::new(mk_quiet_report()));

    gen.generate_metadata(&mut restorer)
}

fn set_needs_check(engine: Arc<dyn IoEngine + Send + Sync>) -> Result<()> {
    use crate::thin::superblock::*;

    let mut sb = read_superblock(engine.as_ref(), SUPERBLOCK_LOCATION)?;
    sb.flags.needs_check = true;
    write_superblock(engine.as_ref(), SUPERBLOCK_LOCATION, &sb)
}

//------------------------------------------

pub enum MetadataOp {
    Format,
    SetNeedsCheck,
}

pub struct ThinGenerateOpts<'a> {
    pub async_io: bool,
    pub op: MetadataOp,
    pub data_block_size: u32,
    pub nr_data_blocks: u64,
    pub output: &'a Path,
}

pub fn generate_metadata(opts: ThinGenerateOpts) -> Result<()> {
    let engine: Arc<dyn IoEngine + Send + Sync> = if opts.async_io {
        Arc::new(AsyncIoEngine::new(opts.output, MAX_CONCURRENT_IO, true)?)
    } else {
        let nr_threads = std::cmp::max(8, num_cpus::get() * 2);
        Arc::new(SyncIoEngine::new(opts.output, nr_threads, true)?)
    };

    match opts.op {
        MetadataOp::Format => format(engine, ThinGenerator),
        MetadataOp::SetNeedsCheck => set_needs_check(engine),
    }
}

//------------------------------------------
