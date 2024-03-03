namespace Monocurl
{
    partial class Landing
    {
        /// <summary>
        ///  Required designer variable.
        /// </summary>
        private System.ComponentModel.IContainer components = null;

        /// <summary>
        ///  Clean up any resources being used.
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
        ///  Required method for Designer support - do not modify
        ///  the contents of this method with the code editor.
        /// </summary>
        private void InitializeComponent()
        {
            newProject = new Button();
            importProject = new Button();
            projectContainer = new GroupBox();
            projectViews = new ListBox();
            forgetProject = new Button();
            label1 = new Label();
            projectContainer.SuspendLayout();
            SuspendLayout();
            // 
            // newProject
            // 
            newProject.Location = new Point(12, 24);
            newProject.Name = "newProject";
            newProject.Size = new Size(120, 29);
            newProject.TabIndex = 1;
            newProject.Text = "New";
            newProject.UseVisualStyleBackColor = true;
            newProject.Click += NewProjectClicked;
            // 
            // importProject
            // 
            importProject.Location = new Point(138, 24);
            importProject.Name = "importProject";
            importProject.Size = new Size(94, 29);
            importProject.TabIndex = 2;
            importProject.Text = "Import";
            importProject.UseVisualStyleBackColor = true;
            importProject.Click += ImportProjectClicked;
            // 
            // projectContainer
            // 
            projectContainer.Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            projectContainer.BackColor = Color.FromArgb(50, 50, 50);
            projectContainer.Controls.Add(projectViews);
            projectContainer.ForeColor = Color.CornflowerBlue;
            projectContainer.Location = new Point(12, 59);
            projectContainer.Name = "projectContainer";
            projectContainer.Size = new Size(862, 379);
            projectContainer.TabIndex = 3;
            projectContainer.TabStop = false;
            projectContainer.Text = "Projects";
            // 
            // projectViews
            // 
            projectViews.Anchor = AnchorStyles.Top | AnchorStyles.Bottom | AnchorStyles.Left | AnchorStyles.Right;
            projectViews.BackColor = Color.FromArgb(50, 50, 50);
            projectViews.BorderStyle = BorderStyle.None;
            projectViews.ForeColor = Color.AliceBlue;
            projectViews.FormattingEnabled = true;
            projectViews.ItemHeight = 20;
            projectViews.Location = new Point(6, 26);
            projectViews.Name = "projectViews";
            projectViews.Size = new Size(850, 340);
            projectViews.TabIndex = 0;
            projectViews.SelectedIndexChanged += projectViews_SelectedIndexChanged;
            projectViews.MouseDoubleClick += projectViews_DoubleClick;
            // 
            // forgetProject
            // 
            forgetProject.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            forgetProject.Location = new Point(751, 24);
            forgetProject.Name = "forgetProject";
            forgetProject.Size = new Size(117, 29);
            forgetProject.TabIndex = 4;
            forgetProject.Text = "Forget Project";
            forgetProject.UseVisualStyleBackColor = true;
            forgetProject.Click += forgetProject_Click;
            // 
            // label1
            // 
            label1.AutoSize = true;
            label1.ForeColor = Color.White;
            label1.Location = new Point(238, 28);
            label1.Name = "label1";
            label1.Size = new Size(157, 20);
            label1.TabIndex = 5;
            label1.Text = "Monocurl (beta v0.1.0)";
            // 
            // Landing
            // 
            AutoScaleDimensions = new SizeF(8F, 20F);
            AutoScaleMode = AutoScaleMode.Font;
            BackColor = Color.FromArgb(28, 28, 28);
            ClientSize = new Size(886, 450);
            Controls.Add(label1);
            Controls.Add(forgetProject);
            Controls.Add(projectContainer);
            Controls.Add(importProject);
            Controls.Add(newProject);
            Name = "Landing";
            Text = "Monocurl";
            Load += Landing_Load;
            projectContainer.ResumeLayout(false);
            ResumeLayout(false);
            PerformLayout();
        }

        #endregion

        private Button newProject;
        private Button importProject;
        private GroupBox projectContainer;
        private ListBox projectViews;
        private Button forgetProject;
        private Label label1;
    }
}