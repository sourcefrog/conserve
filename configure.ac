AC_PREREQ(2.53)
AC_INIT([conserve],[0.1.0])
AM_INIT_AUTOMAKE
AC_PROG_CXX

# pthreads aren't directly used by Conserve but seem to be linked by protobuf.
AC_CHECK_LIB(pthread, pthread_join)

AC_CONFIG_FILES([
  Makefile
  ])
AC_OUTPUT
