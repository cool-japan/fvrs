use native_windows_gui as nwg;

#[derive(Debug)]
pub enum GuiError {
    Nwg(nwg::NwgError),
    Io(std::io::Error),
    InvalidOperation(String),
}

impl From<nwg::NwgError> for GuiError {
    fn from(err: nwg::NwgError) -> Self {
        GuiError::Nwg(err)
    }
}

impl From<std::io::Error> for GuiError {
    fn from(err: std::io::Error) -> Self {
        GuiError::Io(err)
    }
}

impl std::fmt::Display for GuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuiError::Nwg(e) => write!(f, "NWG error: {}", e),
            GuiError::Io(e) => write!(f, "IO error: {}", e),
            GuiError::InvalidOperation(s) => write!(f, "Invalid operation: {}", s),
        }
    }
}

impl std::error::Error for GuiError {}

pub type GuiResult<T> = Result<T, GuiError>;

#[derive(Default)]
pub struct FileVisorApp {
    window: nwg::Window,
    
    // メニュー
    menu: nwg::Menu,
    file_menu: nwg::MenuItem,
    
    // ツールバー
    toolbar: nwg::Frame,
    back_btn: nwg::Button,
    forward_btn: nwg::Button,
    up_btn: nwg::Button,
    
    // アドレスバー
    address_label: nwg::Label,
    address_bar: nwg::TextInput,
    
    // ツリービュー
    tree_view: nwg::TreeView,
    
    // ファイル一覧
    file_list: nwg::ListView,
    
    // ステータスバー
    status_bar: nwg::StatusBar,
}

impl FileVisorApp {
    fn build_ui(&mut self) -> GuiResult<()> {
        // メインウィンドウ
        nwg::Window::builder()
            .title("FVRS - FileVisor風ファイルマネージャー")
            .size((1000, 700))
            .position((100, 100))
            .build(&mut self.window)?;

        // メニューバー
        nwg::Menu::builder()
            .popup(false)
            .parent(&self.window)
            .build(&mut self.menu)?;

        nwg::MenuItem::builder()
            .text("ファイル(&F)")
            .parent(&self.menu)
            .build(&mut self.file_menu)?;

        // ツールバー
        nwg::Frame::builder()
            .parent(&self.window)
            .position((0, 30))
            .size((1000, 40))
            .build(&mut self.toolbar)?;

        nwg::Button::builder()
            .text("←")
            .parent(&self.toolbar)
            .position((10, 5))
            .size((30, 25))
            .build(&mut self.back_btn)?;

        nwg::Button::builder()
            .text("→")
            .parent(&self.toolbar)
            .position((45, 5))
            .size((30, 25))
            .build(&mut self.forward_btn)?;

        nwg::Button::builder()
            .text("↑")
            .parent(&self.toolbar)
            .position((80, 5))
            .size((30, 25))
            .build(&mut self.up_btn)?;

        // アドレスバー
        nwg::Label::builder()
            .text("アドレス:")
            .parent(&self.window)
            .position((10, 75))
            .size((60, 25))
            .build(&mut self.address_label)?;

        nwg::TextInput::builder()
            .text("C:\\")
            .parent(&self.window)
            .position((75, 75))
            .size((900, 25))
            .build(&mut self.address_bar)?;

        // ツリービュー（左側）
        nwg::TreeView::builder()
            .parent(&self.window)
            .position((10, 110))
            .size((250, 500))
            .build(&mut self.tree_view)?;

        // ファイル一覧（右側）
        nwg::ListView::builder()
            .parent(&self.window)
            .position((270, 110))
            .size((720, 500))
            .list_style(nwg::ListViewStyle::Detailed)
            .ex_flags(nwg::ListViewExFlags::FULL_ROW_SELECT)
            .build(&mut self.file_list)?;

        // ファイル一覧のカラム設定
        self.file_list.insert_column("名前");
        self.file_list.insert_column("サイズ");
        self.file_list.insert_column("種類");
        self.file_list.insert_column("更新日時");

        // ステータスバー
        nwg::StatusBar::builder()
            .parent(&self.window)
            .text("準備完了")
            .build(&mut self.status_bar)?;

        Ok(())
    }

    fn init_data(&mut self) -> GuiResult<()> {
        // ツリービューの初期化
        let root = self.tree_view.insert_item("マイコンピュータ", None, nwg::TreeInsert::Root);
        let c_drive = self.tree_view.insert_item("ローカルディスク (C:)", Some(&root), nwg::TreeInsert::Last);
        self.tree_view.insert_item("ドキュメント", Some(&c_drive), nwg::TreeInsert::Last);
        self.tree_view.insert_item("ダウンロード", Some(&c_drive), nwg::TreeInsert::Last);
        self.tree_view.insert_item("デスクトップ", Some(&c_drive), nwg::TreeInsert::Last);
        
        // ファイル一覧の初期化
        let sample_files = vec![
            ("フォルダ1", "", "フォルダ", "2025/06/07 12:00"),
            ("ファイル1.txt", "1.2 KB", "テキストファイル", "2025/06/07 11:30"),
            ("ファイル2.rs", "2.5 KB", "Rustソースファイル", "2025/06/07 10:15"),
            ("画像.png", "340 KB", "PNG画像", "2025/06/07 09:45"),
        ];

        for (i, (name, size, file_type, modified)) in sample_files.iter().enumerate() {
            self.file_list.insert_item(nwg::InsertListViewItem {
                index: Some(i as i32),
                column_index: 0,
                text: Some(name.to_string()),
                image: None,
            });
            
            // サブアイテムを追加
            self.file_list.insert_items_row(
                Some(i as i32),
                &[
                    nwg::InsertListViewItem {
                        index: None,
                        column_index: 1,
                        text: Some(size.to_string()),
                        image: None,
                    },
                    nwg::InsertListViewItem {
                        index: None,
                        column_index: 2,
                        text: Some(file_type.to_string()),
                        image: None,
                    },
                    nwg::InsertListViewItem {
                        index: None,
                        column_index: 3,
                        text: Some(modified.to_string()),
                        image: None,
                    },
                ]
            );
        }

        self.status_bar.set_text(0, &format!("{}個のアイテム", sample_files.len()));

        Ok(())
    }
}

fn main() -> GuiResult<()> {
    // NWGの初期化
    nwg::init()?;
    
    // アプリケーションの作成
    let mut app = FileVisorApp::default();
    app.build_ui()?;
    app.init_data()?;
    
    // ウィンドウの表示
    app.window.set_visible(true);
    
    // イベントループ
    nwg::dispatch_thread_events();
    
    Ok(())
} 