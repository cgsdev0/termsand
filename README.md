# termsand

this is VERY rough and hacked together in a couple hours

it may not work exactly right for you

![sand3-small](https://github.com/user-attachments/assets/3763fef3-3ba6-4532-887e-f5a61fe2b221)

## how do i use it

this thing is designed to work with tmux

1. `cargo install --git https://github.com/cgsdev0/termsand`
2. save this bash script to somewhere (e.g. `sand.sh`):

```bash
#!/bin/bash

popup() {
  TMP="$(mktemp)"
  trap "rm $TMP" EXIT
  cat > "$TMP"
  tmux display-popup -w $WIDTH -h $HEIGHT $* "cat \"$TMP\" | termsand"
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
```
3. bind it to a key in your `tmux.conf` like this:
```
bind-key e run-shell "./sand.sh"
```

and then maybe it will work

good luck have fun

# More GIFs

![sand](https://github.com/user-attachments/assets/fbaa4c60-1f19-4795-9bee-2b7d9a2c23be)

![sand2](https://github.com/user-attachments/assets/de13ac09-a753-44c7-8557-eb81a95f1788)

![sand3](https://github.com/user-attachments/assets/63757ff1-14e7-42ab-8132-c9c339c449ca)
