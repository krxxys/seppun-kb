echo "You must need to have rust, xkbcommon, x11 installed"
cargo build --release
sudo cp ./target/release/seppun-kb /bin/
echo "Watch examples to see how to configure :), have a good day!"
echo "Creating config dir"
mkdir $HOME/.config/seppun/
echo "Copying example configuration"
cp ./examples/kb $HOME/.config/seppun/kb
echo "Installation complete."
echo "Edit your configuration file in $HOME/.config/seppun/kb"
echo "And start seppun using seppun-kb start"
