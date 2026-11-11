namespace GameOffsets.Objects.States
{
    using System;
    using System.Runtime.InteropServices;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct InGameStateOffset
    {
        [FieldOffset(0x218)] public IntPtr AreaInstanceData;
        [FieldOffset(0x270)] public IntPtr WorldData;
        [FieldOffset(0x5A0)] public IntPtr UiRootPtr;
        [FieldOffset(0x978)] public IntPtr IngameUi;
    }
}
