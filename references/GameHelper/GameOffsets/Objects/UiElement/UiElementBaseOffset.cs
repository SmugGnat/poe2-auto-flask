namespace GameOffsets.Objects.UiElement
{
    using System;
    using System.Runtime.InteropServices;
    using Natives;

    [StructLayout(LayoutKind.Explicit, Pack = 1)]
    public struct UiElementBaseOffset
    {
        // exe 05.09.2026: 0x90 bytes were added in front of Self, another 0x08 before ParentPtr
        // and more before UnscaledSize. All values below measured on root/IngameUi/life_orb/mana_orb.
        [FieldOffset(0x000)] public IntPtr Vtable;
        [FieldOffset(0x0B8)] public IntPtr Self;

        [FieldOffset(0x0C0)] public StdVector ChildrensPtr; // both points to same children UiElements.
        // 4 childrens are cached here (0xD8..0xF0)
        // public StdVector ChildrensPtr; // both points to same children UiElements. (0xF8)
        // 4 childrens are cached here (0x110..0x128)

        [FieldOffset(0x138)] public StdTuple2D<float> PositionModifier;
        [FieldOffset(0x140)] public byte ScaleIndex; // root = 3, child of root = 3 and first child of that child = 2
        // Following Ptr is basically pointing to InGameState+0x3A8.
        // [FieldOffset(0x148)] public IntPtr UnknownPtr;

        [FieldOffset(0x150)] public StdTuple2D<float> RelativePosition; // variable

        [FieldOffset(0x168)] public float LocalScaleMultiplier;

        [FieldOffset(0x1B8)] public StdWString StringIdPtr;
        [FieldOffset(0x1E0)] public IntPtr ParentPtr; // UiElement.
        [FieldOffset(0x1E8)] public uint Flags; // variable

        [FieldOffset(0x268)] public StdTuple2D<float> UnscaledSize; // variable

        // exe 05.09.2026: measured on the chat panel, alpha toggles 0x00 <-> 0x8C with chat closed/open.
        [FieldOffset(0x288)] public uint BackgroundColor;
    }

    public static class UiElementBaseFuncs
    {
        private const int SHOULD_MODIFY_BINARY_POS = 0x0A;
        private const int IS_VISIBLE_BINARY_POS = 0x0B;

        /// <summary>
        ///     This value is extracted from GGPK -> Metadata -> UI -> UISettings.xml
        /// </summary>
        public static readonly StdTuple2D<double> BaseResolution = new(2560, 1600);

        public static Func<uint, bool> IsVisibleChecker = param =>
        {
            return Util.isBitSetUint(param, IS_VISIBLE_BINARY_POS);
        };

        public static Func<uint, bool> ShouldModifyPos = param =>
        {
            return Util.isBitSetUint(param, SHOULD_MODIFY_BINARY_POS);
        };
    }
}