A hacky program to give schedule information for Madison Metro buses.

This program uses schedule data and accesses the real-time data, which are
offered by MMT. If there is no internet access, one will see a warning and only
the static scheduling info is displayed.

Using this program implies that you accept MMT's Terms here:
http://transitdata.cityofmadison.com/MetroTransitDataTermsOfUse.pdf

# Building

Requires Rust stable 1.36 or later: https://rustup.rs

```
cargo run # shows help
```

Optionally, you can set the `BUS_DATA` environment variable to the location of
the bus data, and install the data and binary to a well-known location:

```console
cd bus
cp -r data $HOME/.bus
export BUS_DATA=$HOME/.bus # add to .bashrc
cargo install --path .
```
