#! /bin/bash -ex

testtmp=`mktemp -d`
archive=$testtmp/arch
./conserve init-archive $archive 
[ -f $archive/CONSERVE-ARCHIVE ]
