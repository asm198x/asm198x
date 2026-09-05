/* Compile against an unmodified SameBoy Core (see docs/symbol-exports.md).
 * This exercises the consumer's actual loader and expression resolver. */
#include "gb.h"
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv)
{
    if (argc != 2) return 2;
    GB_gameboy_t *gb = calloc(1, sizeof(*gb));
    if (!gb) return 2;
    GB_init(gb, GB_MODEL_DMG_B);
    GB_debugger_load_symbol_file(gb, argv[1]);
    uint16_t address = 0, bank = 0;
    bool found = !GB_debugger_evaluate(gb, "Banked", &address, &bank);
    printf("Banked: found=%d bank=%u address=%04x\n", found, bank, address);
    bool ok = found && bank == 3 && address == 0x4010;
    GB_free(gb);
    free(gb);
    return ok ? 0 : 1;
}
