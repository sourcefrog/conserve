#! /bin/bash -ex

testtmp=`mktemp -d`
archive=$testtmp/arch

diff -u <(./conserve -V) - <<EOF
conserve 0.0
EOF

./conserve -h

./conserve init-archive $archive 
[ -f $archive/CONSERVE-ARCHIVE ]
