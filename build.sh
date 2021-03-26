#!/bin/bash
HW_PATH=$(realpath `dirname "$0"`/hw)
mkdir $HW_PATH/build
rm -rf $HW_PATH/dist
mkdir $HW_PATH/dist
cd $HW_PATH/build
cmake -DCMAKE_BUILD_TYPE="Release" -DNOSERVER=1 -DNOVIDEOREC=1 -DNOPNG=1 -DLUA_SYSTEM=0 -OFFICIAL_SERVER=0 -DDATA_INSTALL_DIR=$HW_PATH/dist ..
make DESTDIR=$HW_PATH/dist install
cd $HW_PATH/dist
mv `find . -name "Data"` .
mv usr/local/bin/* .
mv usr/local/lib/* .
rm -rf usr
rm -rf home
cd $HW_PATH
tar --zstd -cf dist.tar.zst dist/