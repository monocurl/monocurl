using Bridge;

namespace Monocurl
{
    public partial class Editor : Form
    {
        private SceneBridge scene;
        private bool presenting = false;
        public Editor(string path)
        {

            InitializeComponent();

            this.Icon = new Icon("res/monocurl.ico");
            unsafe
            {
                this.scene = new SceneBridge(
                    path,
                    this,
                    this.rootContainer,
                    this.slides,
                    this.mediaList,
                    this.timelineViewportContainer.Panel2,
                    this.timelineViewportContainer.Panel1
                );
            }
            this.FormClosing += this.FreeResources;
            this.scene.reroot_scene();

            this.CenterToScreen();
        }

        /* https://stackoverflow.com/a/738767 */
        public Control FindFocusedControl()
        {
            Control control = ActiveControl;
            ContainerControl container = control as ContainerControl;

            while (container != null)
            {
                control = container.ActiveControl;
                container = control as ContainerControl;
            }
            return control;
        }

        /* https://stackoverflow.com/a/8868761 */
        private void GoFullscreen(bool fullscreen)
        {
            if (fullscreen)
            {
                this.WindowState = FormWindowState.Normal;
                this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.None;
                this.Bounds = Screen.PrimaryScreen.Bounds;
            }
            else
            {
                this.WindowState = FormWindowState.Maximized;
                this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.Sizable;
            }
        }

        protected override bool ProcessCmdKey(ref Message msg, Keys keyData)
        {
            if (FindFocusedControl() is not TextBox)
            {
                if (keyData == Keys.Space)
                {
                    this.scene.toggle_play();
                    return true;
                }
                else if (keyData == Keys.Oemcomma)
                {
                    this.scene.prev_slide();
                    return true;
                }
                else if (keyData == Keys.OemPeriod)
                {
                    this.scene.next_slide();
                    return true;
                }
                else if (keyData == (Keys.Oemcomma | Keys.Shift))
                {
                    this.scene.scene_start();
                    return true;
                }
                else if (keyData == (Keys.OemPeriod | Keys.Shift))
                {
                    this.scene.scene_end();
                    return true;
                }
                else if (keyData == Keys.Escape && presenting)
                {
                    this.toggle();
                    return true;
                }
                else if (keyData == (Keys.F | Keys.Control))
                {
                    this.toggle();
                    return true;
                }
            }

            return base.ProcessCmdKey(ref msg, keyData);
        }

        private void FreeResources(object? sender, System.ComponentModel.CancelEventArgs e)
        {
            this.scene.Dispose();
        }

        private void addMediaButton_Click(object sender, EventArgs e)
        {
            OpenFileDialog openFileDialog = new();
            openFileDialog.Title = "Select Media Path";
            openFileDialog.Filter = "Image Files|*.jpg;*.jpeg;*.png;";
            openFileDialog.CheckFileExists = true;

            if (openFileDialog.ShowDialog() == DialogResult.OK)
            {
                scene.insert_media_image(openFileDialog.FileName);
            }
        }

        private void saveToolStripMenuItem_Click(object sender, EventArgs e)
        {
            scene.force_save();
        }

        private void prevSlide_onClick(object sender, EventArgs e)
        {
            scene.prev_slide();
        }

        private void nextSlide_onClick(object sender, EventArgs e)
        {
            scene.next_slide();
        }

        private void sceneEnd_onClick(object sender, EventArgs e)
        {
            scene.scene_end();
        }
        private void sceneStart_Click(object sender, EventArgs e)
        {
            this.scene.scene_start();
        }

        private void playToolStripMenuItem_Click(object sender, EventArgs e)
        {
            this.scene.toggle_play();
        }

        private void addSlideButton_Click(object sender, EventArgs e)
        {
            scene.add_slide();
        }

        private void insertSlideToolStripMenuItem_Click(object sender, EventArgs e)
        {
            scene.add_slide();
        }

        private void togglePresentationToolStripMenuItem_Click(object sender, EventArgs e)
        {
            this.toggle();
        }

        private void toggle()
        {
            scene.toggle_presentation();
            presenting = !presenting;
            GoFullscreen(presenting);
        }
    }
}
