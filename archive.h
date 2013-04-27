#ifndef CONSERVE_ARCHIVE_H_
#define CONSERVE_ARCHIVE_H_

#include "string"

namespace conserve {

using namespace std;

class Archive {
public:
    static Archive* create(const string base_dir);

private:
    const string base_dir_;

    Archive(const string base_dir) :
	base_dir_(base_dir)
	{}
};

} // namespace conserve
#endif // CONSERVE_ARCHIVE_H_
