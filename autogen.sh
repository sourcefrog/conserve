#! /bin/sh -e

# 'missing' may be missing on the first run in a bare tree
[ -f missing ] && MISSING="./missing --run" || MISSING=

$MISSING aclocal
$MISSING automake --foreign --add-missing
$MISSING autoconf
./configure
