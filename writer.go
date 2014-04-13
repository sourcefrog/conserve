package conserve

func AssertNotFinished(finished bool) {
    if finished {
        panic("Writer has finished")
    }
}
