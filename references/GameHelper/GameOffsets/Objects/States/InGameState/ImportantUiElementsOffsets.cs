namespace GameOffsets.Objects.States.InGameState
{
    using System;
    using System.Runtime.InteropServices;

    /// <summary>
    ///     All offsets over here are UiElements.
    /// </summary>
    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct ImportantUiElementsOffsets
    {
        [FieldOffset(0x540)] public IntPtr ChatParentPtr; // exe 05.09.2026: measured with chat open/close
        [FieldOffset(0x650)] public IntPtr PassiveSkillTreePanel; // exe 05.09.2026: measured with the tree open
        [FieldOffset(0x6E0)] public IntPtr MapParentPtr; // exe 05.09.2026: id "map_overlay"
        [FieldOffset(0x9C8)] public IntPtr ControllerModeMapParentPtr; // exe 05.09.2026: measured 14.10.2026 in controller mode (container without id, map at its LargeMapPtr slot); 0x7A8 was the 2024 value
    }

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct MapParentStruct
    {
        [FieldOffset(0xD8)] public IntPtr LargeMapPtr; // 1st child ~ reading from cache location
        [FieldOffset(0xE0)] public IntPtr MiniMapPtr; // 2nd child ~ reading from cache location
    }

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct PassiveSkillTreeStruct
    {
        [FieldOffset(0xE0)] public IntPtr SkillTreeNodeUiElements; // 2nd child ~ reading from cache location
    }
}
