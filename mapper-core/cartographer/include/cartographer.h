// Free a string returned by cartographer_* functions.
// Returns: 0 on success, -1 if double-free detected, -2 if invalid pointer.
int32_t cartographer_free_string(char* ptr);
