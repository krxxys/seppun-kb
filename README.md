
# seppun-kb

seppun-kb is yet another X hotkey daemon inspired by shxkd, written with rust :)


## Features
- Human like language configuration
- Ability to add and reload binds by hand using cli


## Installation
1. Have rust, x11, xkb-common installed
```
sh ./install.sh
```
2. Create config dir in $HOME/.config/ and copy the example config to desired location
```
mkdir ~/.config/seppun/
cp ./examples/kb ~/.config/seppun/
```
3. Start the app
```
seppun-kb start //starts app
seppun-kb stop  //stops app
seppun-kb reload //reloads the config
seppun-kb config add mod4+return=kitty // add bind
```

## Related

Here are some related projects

- [sxhkd](https://github.com/baskerville/sxhkd)


## License

[MIT](https://choosealicense.com/licenses/mit/)
# seppun-kb
