#!/bin/bash

ARGS="$@"

popup() {
  TMP="$(mktemp)"
  trap "rm $TMP" EXIT
  cat > "$TMP"
  tmux display-popup -w $WIDTH -h $HEIGHT $* "cat \"$TMP\" | termsand $ARGS"
}

PANE=$(tmux display -p "#{pane_id}")
DIMENSIONS="$(tmux display -p "#{pane_width}x#{pane_height}")"
POSITION="$(tmux display -p "#{pane_left}x#{pane_top}")"
HEIGHT="${DIMENSIONS#*x}"
WIDTH="${DIMENSIONS%x*}"
Y="${POSITION#*x}"
((Y+=HEIGHT+1))
X="${POSITION%x*}"
STUFF="$(tmux capture-pane -p -e -t "$PANE")"
echo -e "$STUFF" \
  | popup -E -B -y$Y -x$X -w $WIDTH -h $HEIGHT
