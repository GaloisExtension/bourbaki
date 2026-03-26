import { useCallback, useEffect, useState } from "react";
import { deleteBook, listBooks } from "../../api/commands";
import "./BookManager.css";

type Book = {
  id: string;
  pdfPath: string;
  pageCount: number | null;
  createdAt: number;
  indexedPages: number;
  embeddedPages: number;
};

type Props = {
  currentBookId: string;
  onSelectBook: (bookId: string, pdfPath: string) => void;
  onAddBook: () => void;
  onClose: () => void;
};

function basename(path: string): string {
  return path.split("/").pop() ?? path;
}

function fmtDate(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString("ja-JP", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function BookManager({ currentBookId, onSelectBook, onAddBook, onClose }: Props) {
  const [books, setBooks] = useState<Book[]>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(() => {
    setLoading(true);
    listBooks()
      .then(setBooks)
      .catch(console.error)
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleDelete = useCallback(
    async (bookId: string) => {
      await deleteBook(bookId);
      refresh();
    },
    [refresh],
  );

  const handleSelect = useCallback(
    (book: Book) => {
      onSelectBook(book.id, book.pdfPath);
      onClose();
    },
    [onSelectBook, onClose],
  );

  return (
    <div className="book-manager-backdrop" onClick={onClose}>
      <div
        className="book-manager-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="book-manager-header">
          <h2 className="book-manager-title">教科書管理</h2>
          <button className="book-manager-close" onClick={onClose}>✕</button>
        </div>

        <div className="book-manager-toolbar">
          <button
            className="book-manager-add-btn"
            onClick={() => { onAddBook(); onClose(); }}
          >
            + 新しいPDFを追加
          </button>
        </div>

        <div className="book-manager-list">
          {loading ? (
            <div className="book-manager-empty">読み込み中…</div>
          ) : books.length === 0 ? (
            <div className="book-manager-empty">
              まだ教科書が登録されていません。
              <br />上の「+ 新しいPDFを追加」から始めてください。
            </div>
          ) : (
            books.map((book) => {
              const isCurrent = book.id === currentBookId;
              return (
                <div
                  key={book.id}
                  className={`book-card ${isCurrent ? "book-card--current" : ""}`}
                >
                  <div className="book-card-info">
                    <div className="book-card-name" title={book.pdfPath}>
                      {basename(book.pdfPath)}
                    </div>
                    <div className="book-card-meta">
                      <span>{book.pageCount != null ? `${book.pageCount}ページ` : "ページ数不明"}</span>
                      <span>索引: {book.indexedPages}p</span>
                      <span>埋め込み: {book.embeddedPages}p</span>
                      <span>{fmtDate(book.createdAt)}</span>
                    </div>
                    <div className="book-card-progress">
                      <div
                        className="book-card-progress-bar"
                        style={{
                          width: book.pageCount
                            ? `${Math.round((book.embeddedPages / book.pageCount) * 100)}%`
                            : "0%",
                        }}
                      />
                    </div>
                  </div>

                  <div className="book-card-actions">
                    {isCurrent ? (
                      <span className="book-card-badge">現在選択中</span>
                    ) : (
                      <button
                        className="book-card-btn book-card-btn--select"
                        onClick={() => handleSelect(book)}
                      >
                        選択
                      </button>
                    )}
                    <button
                      className="book-card-btn book-card-btn--delete"
                      disabled={isCurrent}
                      onClick={() => handleDelete(book.id)}
                      title={isCurrent ? "現在選択中の書籍は削除できません" : "削除"}
                    >
                      削除
                    </button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
