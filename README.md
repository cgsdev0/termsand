# termsand

this is VERY rough and hacked together in a couple hours

it may not work exactly right for you

that said, here's some GIFs

![sand](https://github.com/user-attachments/assets/fbaa4c60-1f19-4795-9bee-2b7d9a2c23be)

![sand2](https://github.com/user-attachments/assets/de13ac09-a753-44c7-8557-eb81a95f1788)

![sand3](https://github.com/user-attachments/assets/63757ff1-14e7-42ab-8132-c9c339c449ca)

## how do i use it

this thing is designed to work with tmux

1. cargo install --git
2. use [this bash script](https://gist.github.com/cgsdev0/c5fd87b0213992bd50194f315296dc98)
3. bind it to a key in your `tmux.conf` like this:
```
bind-key e run-shell "./sand.sh sand"
```

and then maybe it will work

good luck have fun
