#!/bin/sh
# Breaks the Scene keys of c4, git, sankey and packet one way at a time, each so that keys depend
# on an element's place or two elements share one, and shows that the type's walk test then fails.
# usage: .agents/skills/verify-rune-pele/scripts/break-walks.sh [name...]   (under `mise exec --`)
# Each break is one exact edit of a source file, undone before the next. With names, only those
# breaks run. Exits 1 if a break does not apply any more, or if its test still passes with it.
set -u
cd "$(dirname "$0")/../../../.." || exit
logs=out/verify/break-walks
mkdir -p "$logs"
if ! cargo build -q -p pele-core --tests > "$logs/unbroken.log" 2>&1; then
  echo "the unbroken tree does not build, so no break can be told apart (see $logs/unbroken.log)"
  exit 1
fi
failed=0

# Puts a file back as it was. cargo goes by modification times, and the copy kept aside is
# older than the build of the broken file, so without the touch cargo would keep that build.
restore() {
  mv "$1.unbroken" "$1"
  touch "$1"
}

# break_one <name> <file> <test target> <test filter> <from> <to>
break_one() {
  name=$1 file=$2 target=$3 filter=$4 from=$5 to=$6
  if [ -n "$only" ] && ! printf ' %s ' "$only" | grep -q " $name "; then
    return
  fi
  cp "$file" "$file.unbroken"
  if ! FROM=$from TO=$to perl -0pi -e 'BEGIN { $n = 0 } $n += s/\Q$ENV{FROM}\E/$ENV{TO}/; END { exit($n == 1 ? 0 : 3) }' "$file"; then
    restore "$file"
    echo "break $name: NOT APPLIED, the text to break is gone from $file"
    failed=1
    return
  fi
  if cargo test -q -p pele-core --test "$target" "$filter" > "$logs/$name.log" 2>&1; then
    echo "break $name: tests still PASS with it; $target $filter does not catch it"
    failed=1
  else
    echo "break $name: $target $filter fails, as it should:"
    grep -E "panicked|assertion|left:|right:|moves keys|not the source" "$logs/$name.log" | head -3 | sed 's/^/    /'
  fi
  restore "$file"
}

only="$*"
src=crates/pele-core/src/diagrams

break_one c4-keyed-by-place $src/c4/render.rs c4_walk a_key_survives_an_edit_elsewhere \
  '        .map(|(alias, make)| {
            let duplicate = seen.entry(alias).or_insert(0);
            let part = out.part(make(alias).ranked(*duplicate));' \
  '        .enumerate()
        .map(|(place, (alias, make))| {
            let duplicate = seen.entry(alias).or_insert(0);
            let part = out.part(make(&JsText::from(place.to_string().as_str())).ranked(*duplicate));'
break_one c4-aliases-unranked $src/c4/render.rs c4_walk every_element_is_drawn_under_a_key_of_its_own \
  'let part = out.part(make(alias).ranked(*duplicate));' \
  'let part = out.part(make(alias).ranked(0));'
break_one git-commit-keyed-by-place $src/git/render.rs git_walk a_key_survives_an_edit_elsewhere \
  'out.part(Entity::node_written_as(&name.name, &commit.id).ranked(name.place).tooltip(tooltip))' \
  'out.part(Entity::node(&JsText::from(commit.seq.to_string().as_str())).tooltip(tooltip))'
break_one git-branches-keyed-alike $src/git/render.rs git_walk every_element_is_drawn_under_a_key_of_its_own \
  'let part = out.part(Entity::group(&model.lanes[i]));' \
  'let part = out.part(Entity::group(&model.lanes[0]));'
break_one git-unnamed-keyed-by-made-up-id $src/git/render.rs git_walk a_commit_with_no_id_added_to_one_branch \
  'let name = if commit.own_id {' \
  'let name = if true {'
break_one git-unnamed-ranked-across-branches $src/git/render.rs git_walk a_commit_with_no_id_added_to_one_branch \
  'quoted.push_units(&commit.branch);' \
  ''
break_one sankey-node-keyed-by-place $src/sankey/render.rs sankey_walk a_key_survives_an_edit_elsewhere \
  'model.nodes.iter().map(|id| out.part(Entity::node(id))).collect();' \
  'model.nodes.iter().enumerate().map(|(i, _)| out.part(Entity::node(&JsText::from(i.to_string().as_str())))).collect();'
break_one sankey-links-unranked $src/sankey/render.rs sankey_walk every_element_is_drawn_under_a_key_of_its_own \
  '.decoded());
                *rank += 1;' \
  '.decoded());'
break_one packet-keyed-by-piece $src/packet/render.rs packet_walk keys_come_from_field_ranges \
  'Entity::node_written_as(&range(&whole[of]), &range(&field.block)).ranked(*rank)' \
  'Entity::node(&range(&field.block))'
break_one packet-pieces-unranked $src/packet/render.rs packet_walk every_element_is_drawn_under_a_key_of_its_own \
  '&range(&field.block)).ranked(*rank));' \
  '&range(&field.block)).ranked(0));'

exit "$failed"
