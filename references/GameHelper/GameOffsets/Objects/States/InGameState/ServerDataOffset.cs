namespace GameOffsets.Objects.States.InGameState
{
    using System;
    using System.Runtime.InteropServices;
    using Natives;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct ServerDataStructure
    {
        public const int SKIP = 0xAD00; // for reducing struct size.
        [FieldOffset(0xC9F8 - SKIP)] public StdVector PlayerInventories; // InventoryArrayStruct (0xB850 before exe 05.09.2026)
    }

    // exe 05.09.2026: element shrank from 0x20 to 0x18 bytes (trailing PAD_1 is gone).
    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct InventoryArrayStruct
    {
        [FieldOffset(0x00)] public int InventoryId;
        [FieldOffset(0x04)] public int PAD_0;
        [FieldOffset(0x08)] public IntPtr InventoryPtr0; // InventoryStruct
        [FieldOffset(0x10)] public IntPtr InventoryPtr1; // this points to 0x10 bytes before InventoryPtr0
    }
}
