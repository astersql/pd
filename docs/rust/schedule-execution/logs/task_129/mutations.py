import pathlib, subprocess
root = pathlib.Path.cwd()
p = root / 'pkg/cluster/collect.rs'
original = p.read_text()
verify = str(root / 'docs/rust/schedule-execution/logs/task_129/verify.py')
variants = {
    'stale-red': ('let stores: Vec<_>', 'let region = event;\n    let stores: Vec<_>'),
    'cancel-red': ('if cancelled() {', 'if cancelled() && false {'),
    'stores-red': ('get_region_stores(&region)', 'get_region_stores(event)'),
}
try:
    for name, (old, new) in variants.items():
        assert old in original
        p.write_text(original.replace(old, new))
        subprocess.run(['python3', verify, name, 'cargo fmt --all', 'cargo test -p astersql-pd-cluster-utils --locked --lib task_129 -- --nocapture'], check=True)
        p.write_text(original)
finally:
    p.write_text(original)
