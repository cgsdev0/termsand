# termsand

termsand is the sand simulation for your terminal that no one asked for
![sand3-small](https://github.com/user-attachments/assets/3763fef3-3ba6-4532-887e-f5a61fe2b221)

## how do i use it?

termsand is intended to work with tmux 3.4+

1. `cargo install termsand`
2. (optional) bind it to a key in your `tmux.conf` like this:
```
bind-key e run-shell termsand
```

## how does it work?

termsand will take input from `stdin` and use that as the initial state for the simulation

If you don't pipe anything in, termsand instead captures your current tmux pane and pipes it into another termsand process inside of a popup, like this:

![tmux_1](https://github.com/user-attachments/assets/42ade40d-b944-4e6c-9313-48a159045b1f)

# More GIFs

![sand](https://github.com/user-attachments/assets/fbaa4c60-1f19-4795-9bee-2b7d9a2c23be)

![sand2](https://github.com/user-attachments/assets/de13ac09-a753-44c7-8557-eb81a95f1788)

![sand3](https://github.com/user-attachments/assets/63757ff1-14e7-42ab-8132-c9c339c449ca)
