#include <iostream> 

#include <google/protobuf/text_format.h>
#include <google/protobuf/io/zero_copy_stream_impl.h>

#include "proto/dura.pb.h"

using namespace std;
using namespace google::protobuf::io;
using namespace google::protobuf;


int main(void) {
    cout << "hello world\n";

    duralib::proto::ArchiveHeader header;
    header.set_magic("dura archive");
    
    FileOutputStream out_stream(0);
    TextFormat::Print(header, &out_stream);

    return 0;
}
