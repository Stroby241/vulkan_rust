#!/bin/bash

sh ./scripts/build_release.sh "$1"

cd ./projects/"$1"/ || exit
perf record --call-graph dwarf ./target/release/"$1"

hotspot ./perf.data