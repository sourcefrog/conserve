#! /bin/bash -ex

testtmp=`mktemp -d`
archive=$testtmp/arch

diff -u <(./conserve -V) - <<EOF
conserve 0.0
EOF

./conserve -h

./conserve init-archive $archive 
[ -f $archive/CONSERVE-ARCHIVE ]

src=$testtmp/src
mkdir $src
echo "hello" > $src/hello

./conserve backup $archive $src/hello
[ -d $archive/b0000 ]   # band directory exists
