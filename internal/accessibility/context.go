package accessibility

type FocusContext struct {
	AppName     string
	BundleID    string
	ElementRole string // AXRole: AXTextField, AXTextArea, AXSearchField, etc.
	Title       string // AXTitle or AXDescription
	Placeholder string // AXPlaceholderValue
	Value       string // AXValue — existing text in field
	PID         int32  // For re-activating the app later
}
