using System.Collections;
using System.Diagnostics;
using System.Text.Json;
using System.Windows.Forms.VisualStyles;
using Bridge;
using Monocurl;

namespace Monocurl
{
    public partial class Landing : Form
    {
        private List<string> paths;
        private int selectedIndex = -1;

        public Landing()
        {
            InitializeComponent();

            this.Icon = new Icon("res/monocurl.ico");
            this.paths = new List<string>();
            this.forgetProject.Enabled = false;
        }

        private void Landing_Load(object sender, EventArgs e)
        {
            this.LoadProjects();
        }

        private static string GetProjectsLocation()
        {
            return Path.Join(Application.UserAppDataPath, "projects.json");
        }

        private void UpdateProjectUI()
        {
            this.projectViews.Items.Clear();
            foreach (string project in this.paths)
            {
                this.projectViews.Items.Add(Path.GetFileNameWithoutExtension(project));
            }
        }

        private void LoadProjects()
        {
            try
            {
                string data = File.ReadAllText(GetProjectsLocation());
                this.paths = JsonSerializer.Deserialize<List<string>>(data) ?? new List<string>();
            }
            catch
            {
                string[] file_names = new string[]
                {
                    "Welcome to Monocurl",
                    "Meshes",
                    "Taylor Series",
                    "Weierstrass",
                    "Monocurl Intro Video",
                    "Simple Text",
                    "Pythagorean Theorem",
                    "Logo",
                    "Simulations",
                    "Triangular Numbers",
                    "Mobius Strip",
                    "Electric Field",
                    "Monotonic Stacks"
                };
                string directory = AppDomain.CurrentDomain.BaseDirectory;

                foreach (string file in file_names)
                {
                    string full = Path.Combine(directory, file + ".mcf");
                    this.paths.Add(full);
                    Debug.WriteLine(full);
                }

                this.UpdateProjects();
            }

            this.UpdateProjectUI();
        }

        private void UpdateProjects()
        {
            /* save */
            if (paths != null)
            {
                try
                {
                    string data = JsonSerializer.Serialize(paths);
                    File.WriteAllText(GetProjectsLocation(), data);
                }
                catch { }
            }

            this.UpdateProjectUI();
        }

        private void NewProjectClicked(object sender, EventArgs e)
        {
            SaveFileDialog saveFileDialog = new();
            saveFileDialog.DefaultExt = Monocurl.NODE_FILE_TYPE;
            saveFileDialog.Filter = Monocurl.NODE_FILE_FILTER;
            if (saveFileDialog.ShowDialog() == DialogResult.OK)
            {
                var res = saveFileDialog.FileName;
                try
                {
                    SceneBridge.init_default_scene(res);
                    this.paths.Add(res);
                    this.UpdateProjects();
                }
                catch
                {
                    MessageBoxButtons buttons = MessageBoxButtons.OK;
                    MessageBox.Show("Failed to generate default scene", "Initialization Error", buttons);
                }
            }
        }

        private void ImportProjectClicked(object sender, EventArgs e)
        {
            OpenFileDialog openFileDialog = new();
            openFileDialog.DefaultExt = Monocurl.NODE_FILE_TYPE;
            openFileDialog.Filter = Monocurl.NODE_FILE_FILTER;
            if (openFileDialog.ShowDialog() == DialogResult.OK)
            {
                var res = openFileDialog.FileName;
                this.paths.Add(res);
                this.UpdateProjects();
            }
        }

        private void forgetProject_Click(object sender, EventArgs e)
        {
            if (this.selectedIndex != -1)
            {
                this.paths.RemoveAt(this.selectedIndex);
                this.UpdateProjects();
                this.selectedIndex = -1;
                this.forgetProject.Enabled = false;
            }
        }

        private void projectViews_SelectedIndexChanged(object sender, EventArgs e)
        {
            this.selectedIndex = this.projectViews.SelectedIndex;
            if (this.selectedIndex < 0)
            {
                this.forgetProject.Enabled = false;
            }
            else
            {
                this.forgetProject.Enabled = true;
            }
        }

        private void projectViews_DoubleClick(object sender, MouseEventArgs e)
        {
            int index = this.projectViews.IndexFromPoint(e.Location);
            if (index >= 0)
            {


                Editor editor = null;
                try
                {
                    editor = new(this.paths[index]);
                }
                catch (Exception ex)
                {
                    MessageBox.Show("Either corrupted/nonexistent file or unsupported version. " + ex.Message.ToString());
                    return;
                }

                string comp = this.paths[index];
                this.paths.RemoveAt(index);
                this.paths.Insert(0, comp);
                this.UpdateProjects();

                this.Hide();
                editor.FormClosing += delegate { 
                    this.Show(); 
                };
                editor.Show();
            }
        }
    }
}
