//! export / import（YAML 明文）——Phase 6 实现。
use anyhow::Result;
use std::path::Path;

pub fn export(_store: &crate::store::Store, _path: &Path) -> Result<()> {
    anyhow::bail!("export 未实现（Phase 6）")
}

pub fn import(_store: &mut crate::store::Store, _path: &Path, _force: bool) -> Result<()> {
    anyhow::bail!("import 未实现（Phase 6）")
}
