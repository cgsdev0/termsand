#!/usr/bin/env bash

ARGS="$@"
cd "${0%/*}"

popup() {
  TMP="$(mktemp)"
  trap "rm $TMP" EXIT
  cat > "$TMP"
  if [[ "$ARGS" == *"--list-colors" ]]; then
    tmux display-popup $* "cat \"$TMP\" | termsand $ARGS && sleep 7"
  else
    tmux display-popup $* "cat \"$TMP\" | termsand $ARGS"
  fi
}

PANE="$(tmux display -p "#{pane_id}")"
read -a GEOMETRY < <(tmux display -p "#{pane_width} #{pane_height} #{pane_left} #{pane_top} #{status}-#{status-position}")
HEIGHT="${GEOMETRY[1]}"
WIDTH="${GEOMETRY[0]}"
Y=$((GEOMETRY[3]+HEIGHT))
if [[ "${GEOMETRY[4]}" = "on-top" ]]; then : $((Y++)); fi
X="${GEOMETRY[2]}"
echo -e "$(tmux capture-pane -CTpet "$PANE")" |
  popup -E -B -y $Y -x $X -w $WIDTH -h $HEIGHT
