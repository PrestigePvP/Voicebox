//go:build !darwin

package accessibility

func GetFocusContext() FocusContext {
	return FocusContext{}
}

func PasteIntoApp(_ int32) {}
