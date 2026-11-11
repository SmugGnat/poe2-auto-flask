namespace GameHelper.RemoteObjects.UiElement
{
    using GameHelper.Cache;
    using ImGuiNET;
    using System;

    /// <summary>
    ///     Points to the Chatbox parent UiElement object.
    /// </summary>
    public class ChatParentUiElement : UiElementBase
    {
        /// <summary>
        ///     Initializes a new instance of the <see cref="ChatParentUiElement" /> class.
        /// </summary>
        /// <param name="address">address to the Chat Parent Ui Element of the game.</param>
        /// <param name="parents">parent cache to use for this Ui Element.</param>
        internal ChatParentUiElement(IntPtr address, UiElementParents parents) :
            base(address, parents) {}

        public bool IsChatActive => this.backgroundColor.W * 255 >= 0x8C;

        /// <summary>
        ///     Converts the <see cref="ChatParentUiElement" /> class data to ImGui.
        /// </summary>
        internal override void ToImGui()
        {
            base.ToImGui();
            ImGui.Text($"IsChatActive: {this.IsChatActive} ({this.backgroundColor.W * 255})");
        }
    }
}
