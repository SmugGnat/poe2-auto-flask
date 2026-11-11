namespace GameOffsets
{
    public struct StaticOffsetsPatterns
    {
        /// <summary>
        /// To find these patterns in Ghidra press S.
        /// </summary>
        public static readonly Pattern[] Patterns =
        {
            // <HowToFindIt>
            // 1: Open CheatEngine and Attach to POE Game
            // 2: Search for String: "InGameState", type: UTF-16 (use case insensitive if you don't find anything in the first try)
            // 3: On all of them, "Find out what accesses this address"
            // 4: Highlight one of the instruction
            // 5: Go through all the registers (e.g. RAX, RBX, RCX) and one of them would be the HashNode Address which points to the InGameState Address.
            // 5.1: To know that this is a HashNode Address make sure that Address + 0x20 points to the "InGameState" key (string)
            // 6: Open HashNode Address in the "Dissect data/structure" window of CheatEngine program.
            // 7: @ 0x08 is the Root HashNode. Copy that value (copy xxxx in i.e. p-> xxxxxx)
            // 7.1: To validate that it's a real Root, 0x019 (byte) would be 1.
            // 8: Pointer scan the value ( which is an address ) you got from step-7 with following configs
            //     Maximum Offset Value: 1024 is enough
            //     Maximum Level: 2 is enough, if you don't find anything increase to 3.
            //     "Allow stack addresses of the first threads": false/unchecked
            //     Rest: doesn't matter
            // 9: Copy the base address and put it in your "Add address manually" button this is your InGameState Address.
            // 10: Do "Find out what accesses this address" and make a pattern out of that function. (pick the one which has smallest offset)
            // </HowToFindIt>
            new(
                "Game States",
                "48 83 EC ?? 48 8B F1 33 ED 48 39 2D ^ ?? ?? ?? ??"
            ),

            // <HowToFindIt>
            // NOTE: This is not a good one, it will 100% break if File Root data structure changes visually and/or how deep is it from static/green address changes
            // 1: Open CheatEngine and Attach to POE Game
            // 2: Search for String: "Mods.dat", type: UTF-16 (case sensitive is fine), Writable: false/unchecked.
            // 3: "Find out what accesses this address"
            // 4: Highlight one of the instruction (make sure the highlighted instruction is in POE memory, not Kernel/Windows-Lib memory).
            // 5: For each static (green) address on the register RAX, RBX and etc do the following
            // 6: Find the value 1 (or 0) in float format. It must be either before or on that green address.
            // 7: Open the address you have found in previous step in disect data window and it should look like following, otherwise go to step-4
            //        0
            //        0
            //        pointer
            //        constant int
            //        4 x more stuff
            //        repeat
            // 8: Around that address +/- 0x10 do "Find out whaat access this address"
            // 9: Whatever instruction you find will be your function which loads the address so make a pattern of it
            // </HowToFindIt>
            // UPDATE (exe 05.09.2026): the 16 buckets are no longer inline in .data. The static is now a
            // pointer (function-local magic static) to a heap array of 16 buckets of 0x40 bytes each.
            // This pattern is the getter: cmp [guard],-1 / jne / call init / ... / mov rax,[rip+root] / ret.
            // Alternative (the init writing the pointer after looping over the 0x40 buckets):
            // "48 83 C3 ?? 48 3B ?? 0F 85 ?? ?? ?? ?? C6 05 ?? ?? ?? ?? 01 EB ?? 33 ?? 48 89 ?? ^ ?? ?? ?? ??"
            new(
                "File Root",
                "83 3D ?? ?? ?? ?? FF 75 ?? E8 ?? ?? ?? ?? 48 8D 0D ?? ?? ?? ?? E8 ?? ?? ?? ?? 48 8D 0D ?? ?? ?? ?? E8 ?? ?? ?? ?? 48 8B 05 ^ ?? ?? ?? ?? 48 83 C4 28 C3"
            ),

            // <HowToFindIt>
            // This one is really simple/old school CE formula.
            // The only purpose of this Counter is to figure out the files-loaded-in-current-area.
            // 1: Open CheatEngine and Attach to POE Game
            // 2: Search for 4 bytes, Search type should be "UnknownInitialValue"
            // 3: Now Change the Area again and again and on every area change do a "Next Scan" with "Increased Value"
            // 4: U will find 2 green addresses at the end of that search list.
            // 5: Pick the smaller one and create pattern for it.
            // 5.1: Normally pattern are created by "What accesses this address/value"
            // 5.2: but in this case the pattern at "What writes to this address/value" is more unique.
            //
            // NOTE: Reason you picked the smaller one in step-5 is because
            //       the bigger one is some other number which increments by 3
            //       every time you move from Charactor Select screen to In Game screen.
            //       Feel free to test this, just to be sure that the Address you are picking
            //       is the correct one.
            // </HowToFindIt>
            // UPDATE (exe 05.09.2026): same "call; call; inc dword [rip+X]" as before, only the bytes after
            // the inc changed. It must use the same numbering as FileInfoValueStruct.AreaChangeCount.
            new(
                "AreaChangeCounter",
                "FF ?? ?? ?? ?? ?? E8 ?? ?? ?? ?? FF 05 ^ ?? ?? ?? ??"
            ),

            // <HowToFindIt>
            // Find player -> Render component -> TerrainHeight.
            // Do "What writes to this address" on terrainheight.
            // This instruction which writes to terrainheight, also writes to 200 different address
            // So let's narrow down the invalid result by putting the following Start Condition to it.
            //      if instruction in step-2 is as following
            //      mov [RAX+C8], xmmm0;
            //      then Start Condition will be (RAX==0xRenderCompAddress+(TerrainheightOffset - C8))
            //      CE can't do +, - on Start condition so calculate it via a calculator. Final condition e.g. (RAX==0x123123123)
            // Go to the top of the function you found in step - 2 (u can right click on the statement and select "select current func" and repeat the last step with exact same condition.
            // In your "Trace and break" window that u got from the last step, 3rd or 4th function from the top will be the function from which this pattern is created.
            //          that function will be the first function in that whole window that has more than 10 instructions, every function before this function will have
            //          2 or 3 or 4 instructions max.
            // </HowToFindIt>
            new(
                "Terrain Rotator Helper", // array length = bigger
                "48 ?? ?? ^ ?? ?? ?? ?? 4f ?? ?? ?? 4c ?? ?? 8b ?? 2b ??"

            ),

            // Go to the caller of the function where you find "Terrain Rotator Helper".
            // This would be passed as a an argument.
            new(
                "Terrain Rotation Selector", // example output array = 00 03 02 01 04 05 06 07 08
                "48 ?? ?? ^ ?? ?? ?? ?? 44 ?? ?? ?? ?? b8 ?? ?? ?? ?? 8b ?? ?? ?? ?? 89 ?? ?? ?? BA ??"
            ),

            // <HowToFindIt>
            // Find what accesses LargeMapUiElement -> UiElementBaseOffset -> RelativePosition.
            // There should be just 1 or 2 instructions that access it
            // Open that instruction in ghidra (let's call this function FOO).
            // Find out who calls function FOO.
            // One of the calling functions would be adding a DAT_XYZ into the return/output/result of function FOO.
            // Make a pattern of that area.
            // </HowToFindIt>
            new(
                "GameCullSize",
                "2B ?? ^ ?? ?? ?? ?? 45 ?? ?? ?? 48 ?? ?? ?? ?? ?? ?? F3 ?? ?? ?? ?? ?? ?? ?? ?? D1"
                )
        };
    }
}
