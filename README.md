A hacky program to give schedule information for Madison Metro buses.

This program only uses schedule data, not real-time updates, which are
offered by MMT via a different API.

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
