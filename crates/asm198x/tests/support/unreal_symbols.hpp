// Headless adapter for the unmodified UnrealSpeccy label loader/storage
// methods. Build instructions and pinned source identity are in
// docs/sjasmplus-symbol-exports.md. This is not a replacement parser.
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cctype>
#include <cstdint>
#include <vector>
#define __cdecl
#define PAGE 0x4000
#define CONSCLR_ERROR 1
#define align_by(a,b) (((uintptr_t)(a) + ((b)-1)) & ~((uintptr_t)((b)-1)))
void color(int) {}
void errmsg(const char *message, const char *value) { std::fprintf(stderr, message, value); }
int ishex(char);
unsigned char hex(char);
struct MON_LABEL { unsigned char *address; unsigned name_offs; };
struct MON_LABELS {
    MON_LABEL *pairs = nullptr;
    unsigned n_pairs = 0;
    char *names = nullptr;
    unsigned names_size = 0;
    ~MON_LABELS() { std::free(pairs); std::free(names); }
    unsigned add_name(char *);
    void clear(unsigned char *, unsigned);
    void sort();
    void add(unsigned char *, char *);
    char *find(unsigned char *);
    unsigned load(char *, unsigned char *, unsigned);
};
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    std::vector<unsigned char> memory(0x20000);
    MON_LABELS labels;
    if (labels.load(argv[1], memory.data(), memory.size()) != 3) return 1;
    const char *expected[] = {"draw", "music", "answer"};
    const unsigned addresses[] = {0x4010, 0xc010, 0xc010};
    for (unsigned i = 0; i < 3; ++i) {
        bool found = false;
        for (unsigned j = 0; j < labels.n_pairs; ++j) {
            if (!std::strcmp(labels.names + labels.pairs[j].name_offs, expected[i])) {
                if (labels.pairs[j].address != memory.data() + addresses[i]) return 1;
                found = true;
            }
        }
        if (!found) return 1;
        std::printf("PASS %s: physical=%06x\n", expected[i], addresses[i]);
    }
    return 0;
}
