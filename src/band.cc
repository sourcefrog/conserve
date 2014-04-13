

void BandWriter::finish() {
    proto::BandTail tail_pb;
    tail_pb.set_band_number(name_);
    populate_stamp(tail_pb.mutable_stamp());
    // TODO(mbp): Write block count
    write_proto_to_file(tail_pb, tail_file_name());
    LOG(INFO) << "finish band in " << band_directory_;
}


int BandWriter::next_block_number() {
    // TODO(mbp): Needs to be improved if the band's partially complete.
    return next_block_number_++;
}


BandReader::BandReader(Archive *archive, string name) :
    Band(archive, name),
    current_block_number_(-1)
{
    read_proto_from_file(head_file_name(), &head_pb_, "band", "head");
    read_proto_from_file(tail_file_name(), &tail_pb_, "band", "tail");
    LOG(INFO) << "start reading band " << head_pb_.band_number();
    CHECK(head_pb_.band_number() == tail_pb_.band_number());
    CHECK(tail_pb_.block_count() >= 0);
}


bool BandReader::done() const {
    return current_block_number_ >= tail_pb_.block_count();
}


BlockReader BandReader::read_next_block() {
    current_block_number_++;
    return BlockReader(directory(), current_block_number_);
}


} // namespace conserve

// vim: sw=4 et
