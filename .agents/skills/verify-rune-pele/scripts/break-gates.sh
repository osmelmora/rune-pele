#!/bin/sh
# Breaks the parity gate, the corpus loader, the oracle's Node, the command line and the
# output-limit error one way at a time, and shows that the test of each then fails.
# usage: .agents/skills/verify-rune-pele/scripts/break-gates.sh [name...]   (under `mise exec --`)
# Each break is one exact edit of a source file, undone before the next. With names, only those
# breaks run. Exits 1 if a break does not apply any more, or if its test still passes with it.
set -u
cd "$(dirname "$0")/../../../.."
logs=out/verify/break-gates
mkdir -p "$logs"
failed=0

# Puts a file back as it was. cargo goes by modification times, and the copy kept aside is
# older than the build of the broken file, so without the touch cargo would keep that build.
restore() {
  mv "$1.unbroken" "$1"
  touch "$1"
}

# break_one <name> <file> <package> <test target> <test filter> <from> <to>
# A test target of `bin` runs the unit tests of the package's binary.
break_one() {
  name=$1 file=$2 package=$3 target=$4 filter=$5 from=$6 to=$7
  if [ -n "$only" ] && ! printf ' %s ' "$only" | grep -q " $name "; then
    return
  fi
  cp "$file" "$file.unbroken"
  FROM=$from TO=$to perl -0pi -e 'BEGIN { $n = 0 } $n += s/\Q$ENV{FROM}\E/$ENV{TO}/; END { exit($n == 1 ? 0 : 3) }' "$file"
  if [ $? -ne 0 ]; then
    restore "$file"
    echo "break $name: NOT APPLIED, the text to break is gone from $file"
    failed=1
    return
  fi
  if [ "$target" = bin ]; then
    selector="--bin pele"
  else
    selector="--test $target"
  fi
  if cargo test -q -p "$package" $selector "$filter" > "$logs/$name.log" 2>&1; then
    echo "break $name: tests still PASS with it; $filter does not catch it"
    failed=1
  else
    echo "break $name: $filter fails, as it should:"
    grep -E "panicked|assertion|left:|right:" "$logs/$name.log" | head -3 | sed 's/^/    /'
  fi
  restore "$file"
}

only="$*"
oracle=crates/pele-oracle/src/lib.rs
cli=crates/pele-cli/src/main.rs

break_one corpus-from-disk $oracle pele-cli cli a_missing_corpus_file \
  '    corpus_types()?
        .into_iter()
' \
  '    corpus_types()?
        .into_iter()
        .filter(|name| dir.join(format!("{name}-docs.json")).exists())
'
break_one corpus-malformed-read-as-nothing $oracle pele-cli cli a_corpus_file_that_is_not_json \
  '.map_err(|e| CorpusError(format!("{}: not a JSON array of sources: {e}", path.display())))?;' \
  '.unwrap_or_else(|_| vec!["pie".to_owned()]);'
break_one corpus-empty-file $oracle pele-cli cli a_corpus_file_with_no_sources \
  'if sources.is_empty() {' \
  'if false && sources.is_empty() {'
break_one gate-pele-errors-pass $cli pele-cli cli a_source_pele_rejects \
  'if self.pele_errors != expect_pele_errors {' \
  'if false && self.pele_errors != expect_pele_errors {'
break_one gate-nothing-compared $cli pele-cli cli a_run_that_compares_nothing \
  'if self.identical + self.different == 0 {' \
  'if false && self.identical + self.different == 0 {'
break_one gate-not-ported $cli pele-cli bin a_source_this_port_does_not_render \
  'if self.not_ported > 0 {' \
  'if false && self.not_ported > 0 {'
break_one out-kept $cli pele-cli cli a_reused_out_folder \
  'std::fs::remove_dir_all(dir).map_err(|e| Exit::cant_write(dir, e))?;' \
  'let _ = std::fs::remove_dir_all(dir.join("pie/0.mmd"));'
break_one out-not-ours-emptied $cli pele-cli cli an_out_folder_pele_parity_did_not_write \
  'if !dir.join(MARK).is_file() {' \
  'if false && !dir.join(MARK).is_file() {'
break_one out-not-a-folder-panics $cli pele-cli cli an_out_path_that_cannot_be_a_folder \
  'Err(e) => return Err(Exit::cant_write(dir, e)),' \
  'Err(e) => panic!("{e}"),'
break_one oracle-crash-panics $oracle pele-cli cli an_oracle_that_crashes \
  'return Err(OracleError(format!("the oracle failed ({}); Node'"'"'s report is above", output.status)));' \
  'panic!("oracle failed: {}", output.status);'
break_one node-from-path $oracle pele-cli cli the_oracle_runs_the_node_mise_names \
  'let mut child = Command::new(&node.path)' \
  'let mut child = Command::new("node")'
break_one node-version-unchecked $oracle pele-cli cli the_oracle_refuses_a_node_other \
  'if version != format!("v{pinned}") {' \
  'if false && version != format!("v{pinned}") {'
break_one option-unknown-ignored $cli pele-cli cli unknown_options_and_options_without_values \
  'return Err(Exit::usage(format!("unknown option {arg}")));' \
  'continue;'
break_one option-value-missing-ignored $cli pele-cli cli unknown_options_and_options_without_values \
  'return Err(Exit::usage(format!("{arg} needs a value")));' \
  'continue;'
break_one limit-alias-dropped crates/pele-core/src/svg.rs pele-core output_limit the_output_limit_error \
  'mermaid_type: Some(parity.found.mermaid),' \
  'mermaid_type: None,'

exit "$failed"
