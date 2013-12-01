package conserve_test

import (
    "io/ioutil"
    "os"
    "testing"

    "github.com/sourcefrog/conserve/conserve"
)

func testDirectory() (string, error) {
    return ioutil.TempDir("", "conserve_test")
}

func TestInitArchive(t *testing.T) {
    testDir, err := testDirectory()
    if err != nil {
        t.Error(err.Error())
    }
    archive, err := conserve.InitArchive(testDir)
    if err != nil {
        t.Error(err.Error())
    }
    if archive == nil {
        t.Error("nil archive returned")
    }
    // TODO(mbp): Check header bytes are as expected
    _, err = os.Stat(testDir)
    if os.IsNotExist(err) {
        t.Error("archive directory does not exist")
    }
}
