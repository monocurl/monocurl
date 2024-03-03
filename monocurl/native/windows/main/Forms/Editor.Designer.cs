namespace Monocurl
{

    class NonMovingPanel: Panel
    {
        protected override Point ScrollToControl(Control activeControl)
        {
            Point pt = this.AutoScrollPosition;
            return pt;
        }
    }

    partial class Editor
    {
        /// <summary>
        /// Required designer variable.
        /// </summary>
        private System.ComponentModel.IContainer components = null;

        /// <summary>
        /// Clean up any resources being used.
        /// </summary>
        /// <param name="disposing">true if managed resources should be disposed; otherwise, false.</param>
        protected override void Dispose(bool disposing)
        {
            if (disposing && (components != null))
            {
                components.Dispose();
            }
            base.Dispose(disposing);
        }

        #region Windows Form Designer generated code

        /// <summary>
        /// Required method for Designer support - do not modify
        /// the contents of this method with the code editor.
        /// </summary>
        private void InitializeComponent()
        {
            rootContainer = new SplitContainer();
            editorTabs = new TabControl();
            editorPage = new TabPage();
            slides = new NonMovingPanel();
            mediaPage = new TabPage();
            addMediaButton = new Button();
            mediaList = new Panel();
            timelineViewportContainer = new SplitContainer();
            ((System.ComponentModel.ISupportInitialize)rootContainer).BeginInit();
            rootContainer.Panel1.SuspendLayout();
            rootContainer.Panel2.SuspendLayout();
            rootContainer.SuspendLayout();
            editorTabs.SuspendLayout();
            editorPage.SuspendLayout();
            mediaPage.SuspendLayout();
            ((System.ComponentModel.ISupportInitialize)timelineViewportContainer).BeginInit();
            timelineViewportContainer.SuspendLayout();
            SuspendLayout();
            // 
            // rootContainer
            // 
            rootContainer.BackColor = Color.FromArgb(12, 12, 12);
            rootContainer.Dock = DockStyle.Fill;
            rootContainer.Location = new Point(0, 0);
            rootContainer.Name = "rootContainer";
            // 
            // rootContainer.Panel1
            // 
            rootContainer.Panel1.BackColor = SystemColors.Control;
            rootContainer.Panel1.Controls.Add(editorTabs);
            // 
            // rootContainer.Panel2
            // 
            rootContainer.Panel2.Controls.Add(timelineViewportContainer);
            rootContainer.Size = new Size(1682, 825);
            rootContainer.SplitterDistance = 777;
            rootContainer.TabIndex = 1;
            // 
            // editorTabs
            // 
            editorTabs.Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            editorTabs.Controls.Add(editorPage);
            editorTabs.Controls.Add(mediaPage);
            editorTabs.Location = new Point(0, 0);
            editorTabs.Margin = new Padding(0);
            editorTabs.Multiline = true;
            editorTabs.Name = "editorTabs";
            editorTabs.Padding = new Point(0, 0);
            editorTabs.SelectedIndex = 0;
            editorTabs.Size = new Size(774, 822);
            editorTabs.TabIndex = 0;
            // 
            // editorPage
            // 
            editorPage.Controls.Add(slides);
            editorPage.Location = new Point(4, 29);
            editorPage.Margin = new Padding(0);
            editorPage.Name = "editorPage";
            editorPage.Size = new Size(766, 789);
            editorPage.TabIndex = 0;
            editorPage.Text = "Editor";
            // 
            // slides
            // 
            slides.AutoScroll = true;
            slides.VerticalScroll.Visible = false;
            slides.BackColor = Color.FromArgb(31, 33, 33);
            slides.Dock = DockStyle.Fill;
            slides.Location = new Point(0, 0);
            slides.Margin = new Padding(0);
            slides.Name = "slides";
            slides.Size = new Size(766, 789);
            slides.TabIndex = 0;
            // 
            // mediaPage
            // 
            mediaPage.BackColor = Color.FromArgb(31, 33, 33);
            mediaPage.Controls.Add(addMediaButton);
            mediaPage.Controls.Add(mediaList);
            mediaPage.Location = new Point(4, 29);
            mediaPage.Margin = new Padding(0);
            mediaPage.Name = "mediaPage";
            mediaPage.Size = new Size(552, 789);
            mediaPage.TabIndex = 1;
            mediaPage.Text = "Media";
            // 
            // addMediaButton
            // 
            addMediaButton.Anchor = AnchorStyles.Bottom | AnchorStyles.Left;
            addMediaButton.BackColor = Color.White;
            addMediaButton.Location = new Point(8, 755);
            addMediaButton.Name = "addMediaButton";
            addMediaButton.Size = new Size(94, 29);
            addMediaButton.TabIndex = 0;
            addMediaButton.Text = "Import";
            addMediaButton.UseVisualStyleBackColor = false;
            addMediaButton.Click += addMediaButton_Click;
            // 
            // mediaList
            // 
            mediaList.BackColor = Color.FromArgb(31, 33, 33);
            mediaList.Dock = DockStyle.Top;
            mediaList.Location = new Point(0, 0);
            mediaList.Margin = new Padding(0);
            mediaList.Name = "mediaList";
            mediaList.Size = new Size(552, 722);
            mediaList.TabIndex = 0;
            // 
            // timelineViewportContainer
            // 
            timelineViewportContainer.BackColor = Color.FromArgb(12, 12, 12);
            timelineViewportContainer.Dock = DockStyle.Fill;
            timelineViewportContainer.Location = new Point(0, 0);
            timelineViewportContainer.Name = "timelineViewportContainer";
            timelineViewportContainer.Orientation = Orientation.Horizontal;
            // 
            // timelineViewportContainer.Panel1
            // 
            timelineViewportContainer.Panel1.BackColor = Color.Black;
            // 
            // timelineViewportContainer.Panel2
            // 
            timelineViewportContainer.Panel2.BackColor = Color.FromArgb(28, 28, 28);
            timelineViewportContainer.Size = new Size(901, 825);
            timelineViewportContainer.SplitterDistance = 453;
            timelineViewportContainer.TabIndex = 0;
            // 
            // Editor
            // 
            AutoScaleDimensions = new SizeF(8F, 20F);
            AutoScaleMode = AutoScaleMode.Font;
            BackColor = SystemColors.Control;
            ClientSize = new Size(1682, 853);
            Controls.Add(rootContainer);
            Name = "Editor";
            Text = "Monocurl";
            rootContainer.Panel1.ResumeLayout(false);
            rootContainer.Panel2.ResumeLayout(false);
            ((System.ComponentModel.ISupportInitialize)rootContainer).EndInit();
            rootContainer.ResumeLayout(false);
            editorTabs.ResumeLayout(false);
            editorPage.ResumeLayout(false);
            mediaPage.ResumeLayout(false);
            ((System.ComponentModel.ISupportInitialize)timelineViewportContainer).EndInit();
            timelineViewportContainer.ResumeLayout(false);
            ResumeLayout(false);
            PerformLayout();
        }

        #endregion

        private SplitContainer rootContainer;
        private SplitContainer timelineViewportContainer;
        private TabControl editorTabs;
        private TabPage editorPage;
        private TabPage mediaPage;
        private Panel mediaList;
        private Button addMediaButton;
        private Panel slides;
    }
}
