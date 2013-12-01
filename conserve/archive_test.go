package conserve_test

import (
    "io/ioutil"
    "os"
    "testing"

    "github.com/sourcefrog/conserve/conserve"
)

func testDirectory() (string, error) {
    return ioutil.TempDir("", "conserve_test_")
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

    f, err := os.Open(testDir + "/CONSERVE")
    if err != nil {
        t.Error("failed to read archive magic: ", err)
        return
    }

    magic := make([]byte, 100)
    n, err := f.Read(magic)
    if err != nil {
        t.Error("failed to read archive magic: ", err)
        return
    }

    var expected_magic = ("\x0a\x18conserve backup archive")
    var got_magic = string(magic[:n])
    if got_magic != expected_magic {
        t.Errorf("wrong archive magic: wanted %q got %q",
            expected_magic, got_magic)
    }
}
