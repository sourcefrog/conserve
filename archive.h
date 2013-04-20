#ifndef DURA_ARCHIVE_H_
#define DURA_ARCHIVE_H_

#include "string"

namespace dura {

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

} // namespace dura
#endif // DURA_ARCHIVE_H_
