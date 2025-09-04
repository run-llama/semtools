#!/usr/bin/env python3
"""
ArXiv Dataset Collection and Organization Script

Collects recent arXiv papers and organizes them into a hierarchical structure
suitable for testing CLI agents. Can optionally download and extract readable
text from HTML versions of papers.

Directory Structure:
- by_category/: Papers organized by arXiv category (contains only fulltext symlinks)
- by_date/: Papers organized by publication date (contains only fulltext symlinks)
- by_author/: Papers organized by author name (contains only fulltext symlinks)
- full_text/: Full text extracts (.txt) when download_source=True

Features:
- Clean hierarchical organization without duplicate metadata files
- Optional HTML source text extraction and cleaning
- Automatic symlinks to full text in organizational folders
- Rate-limited API access respecting arXiv guidelines

Dependencies:
- arxiv: For API access (pip install arxiv)
- beautifulsoup4: For HTML text extraction (pip install beautifulsoup4)
"""

import arxiv
import json
import os
import re
import time
import shutil
from datetime import datetime, timedelta
from pathlib import Path
from typing import Dict, List, Set
from urllib.parse import urljoin
import requests
from dataclasses import dataclass, asdict


@dataclass
class PaperMetadata:
    """Structured metadata for each paper"""
    arxiv_id: str
    title: str
    authors: List[str]
    abstract: str
    categories: List[str]
    primary_category: str
    published: str
    updated: str
    doi: str
    pdf_url: str
    entry_id: str
    comment: str
    journal_ref: str


class ArxivDatasetBuilder:
    def __init__(self, base_dir: str = "arxiv_dataset", delay: float = 3.0, download_source: bool = False):
        """
        Initialize the dataset builder.
        
        Args:
            base_dir: Root directory for the dataset
            delay: Delay between API calls (arXiv asks for 3+ seconds)
            download_source: Whether to download and extract source text from LaTeX files
        """
        self.base_dir = Path(base_dir)
        self.delay = delay
        self.download_source = download_source
        self.papers_collected = 0
        self.errors = []
        
        # Create directory structure
        self.setup_directories()
    
    def setup_directories(self):
        """Create the hierarchical directory structure"""
        dirs_to_create = [
            "by_category",
            "by_date", 
            "by_author",
            "full_text"
        ]
        
        for dir_name in dirs_to_create:
            (self.base_dir / dir_name).mkdir(parents=True, exist_ok=True)
        
        print(f"Created dataset structure in {self.base_dir}")
    
    def clean_filename(self, text: str, max_length: int = 100) -> str:
        """Clean text for use as filename"""
        # Remove or replace problematic characters
        cleaned = re.sub(r'[<>:"/\\|?*]', '_', text)
        cleaned = re.sub(r'\s+', '_', cleaned)
        cleaned = cleaned.strip('._')
        
        # Truncate if too long
        if len(cleaned) > max_length:
            cleaned = cleaned[:max_length-3] + "..."
        
        return cleaned
    
    def extract_date_from_id(self, arxiv_id: str) -> str:
        """Extract date from arXiv ID for organization"""
        # Handle both old format (math.CO/0501001) and new format (2107.12345)
        if '/' in arxiv_id:
            # Old format - extract year and month from paper number
            parts = arxiv_id.split('/')
            paper_num = parts[1]
            year = "20" + paper_num[:2] if paper_num[:2] < "50" else "19" + paper_num[:2]
            month = paper_num[2:4]
            return f"{year}-{month}"
        else:
            # New format - first 4 digits are year/month
            if len(arxiv_id) >= 4:
                year_month = arxiv_id[:4]
                year = "20" + year_month[:2]
                month = year_month[2:4]
                return f"{year}-{month}"
        return "unknown"
    
    def save_paper_metadata(self, paper: arxiv.Result) -> PaperMetadata:
        """Save paper metadata in structured format"""
        # Extract clean metadata
        metadata = PaperMetadata(
            arxiv_id=paper.entry_id.split('/')[-1],
            title=paper.title,
            authors=[author.name for author in paper.authors],
            abstract=paper.summary.replace('\n', ' ').strip(),
            categories=paper.categories,
            primary_category=paper.primary_category,
            published=paper.published.isoformat(),
            updated=paper.updated.isoformat() if paper.updated else "",
            doi=paper.doi or "",
            pdf_url=paper.pdf_url,
            entry_id=paper.entry_id,
            comment=paper.comment or "",
            journal_ref=paper.journal_ref or ""
        )
        
        return metadata
    
    def organize_by_category(self, metadata: PaperMetadata):
        """Organize papers by category"""
        for category in metadata.categories:
            cat_dir = self.base_dir / "by_category" / category
            cat_dir.mkdir(parents=True, exist_ok=True)
    
    def organize_by_date(self, metadata: PaperMetadata):
        """Organize papers by publication date"""
        date_str = self.extract_date_from_id(metadata.arxiv_id)
        date_dir = self.base_dir / "by_date" / date_str
        date_dir.mkdir(parents=True, exist_ok=True)
    
    def organize_by_author(self, metadata: PaperMetadata):
        """Organize papers by author"""
        for author in metadata.authors:
            clean_author = self.clean_filename(author)
            author_dir = self.base_dir / "by_author" / clean_author
            author_dir.mkdir(parents=True, exist_ok=True)
    
    def download_source_text(self, paper: arxiv.Result, metadata: PaperMetadata):
        """Download and extract readable text from HTML version"""
        import urllib.request
        import tempfile
        
        # Check if text file already exists
        text_file = self.base_dir / "full_text" / f"{metadata.arxiv_id}.txt"
        if text_file.exists():
            print(f"  Full text already exists for {metadata.arxiv_id}, skipping download")
            return True
        
        # Get HTML URL (replace /abs/ with /html/)
        html_url = paper.entry_id.replace('/abs/', '/html/')
        
        try:
            with tempfile.TemporaryDirectory() as temp_dir:
                # Download HTML
                html_path = f"{temp_dir}/paper.html"
                urllib.request.urlretrieve(html_url, html_path)
                
                # Extract clean text from HTML
                clean_text = self.extract_text_from_html(html_path)
                
                if clean_text:
                    # Check text length (count lines)
                    line_count = len(clean_text.splitlines())
                    
                    if line_count < 500:
                        print(f"  Skipping {metadata.arxiv_id}: too short ({line_count} lines, minimum 500)")
                        return False
                    elif line_count > 10000:
                        print(f"  Skipping {metadata.arxiv_id}: too long ({line_count} lines, maximum 10000)")
                        return False
                    
                    print(f"  Text length acceptable: {line_count} lines")
                    
                    # Save as readable text file
                    with open(text_file, 'w', encoding='utf-8') as f:
                        f.write(clean_text)
                    
                    return True
        except Exception as e:
            print(f"Failed to download HTML source for {metadata.arxiv_id}: {str(e)}")
            return False
        return False
    
    def extract_text_from_html(self, html_file: str) -> str:
        """Extract clean text from arXiv HTML file with preserved formatting"""
        try:
            from bs4 import BeautifulSoup
        except ImportError:
            print("BeautifulSoup4 is required for HTML text extraction. Install with: pip install beautifulsoup4")
            return ""
        
        try:
            with open(html_file, 'r', encoding='utf-8') as f:
                html_content = f.read()
        except:
            return ""
        
        soup = BeautifulSoup(html_content, 'html.parser')
        
        # Remove script and style elements
        for script in soup(["script", "style"]):
            script.decompose()
        
        # Remove navigation and footer elements that are common in arXiv HTML
        for elem in soup.find_all(['nav', 'header', 'footer']):
            elem.decompose()
        
        # Find the main content area (arXiv HTML has specific structure)
        main_content = soup.find('div', {'id': 'content'}) or soup.find('div', {'class': 'ltx_page_main'}) or soup.body
        
        if not main_content:
            main_content = soup
        
        # Process different elements to preserve structure
        text_parts = []
        
        # Extract title
        title = main_content.find(['h1', 'title']) or soup.find('title')
        if title:
            title_text = title.get_text().strip()
            if title_text and not title_text.startswith('['):
                text_parts.append(f"TITLE: {title_text}\n")
        
        # Process paragraphs, headings, and other block elements
        for element in main_content.find_all(['p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'div', 'section']):
            element_text = element.get_text().strip()
            
            if not element_text:
                continue
            
            # Skip navigation elements and short non-content text
            if (len(element_text) < 10 and 
                any(skip in element_text.lower() for skip in ['skip', 'menu', 'nav', 'search', 'login'])):
                continue
            
            # Format headings
            if element.name in ['h1', 'h2', 'h3', 'h4', 'h5', 'h6']:
                text_parts.append(f"\n{element_text.upper()}\n")
            else:
                # Add paragraph breaks for content
                text_parts.append(f"{element_text}\n")
        
        # If we didn't get much structured content, fall back to simpler extraction
        if len(''.join(text_parts)) < 500:
            text = main_content.get_text()
            # Clean up the text but preserve line breaks better
            lines = []
            for line in text.splitlines():
                line = line.strip()
                if line:
                    lines.append(line)
            text = '\n'.join(lines)
        else:
            text = '\n'.join(text_parts)
        
        # Clean up common arXiv HTML artifacts
        import re
        text = re.sub(r'Skip to main content', '', text)
        text = re.sub(r'Cornell University.*?arXiv', '', text)
        text = re.sub(r'\[Submitted on.*?\]', '', text)
        text = re.sub(r'\[v\d+\].*?$', '', text, flags=re.MULTILINE)
        
        # Clean up excessive whitespace while preserving paragraph breaks
        text = re.sub(r'\n\s*\n\s*\n+', '\n\n', text)  # Multiple line breaks to double
        text = re.sub(r'[ \t]+', ' ', text)  # Multiple spaces to single
        text = text.strip()
        
        return text
     
    def create_fulltext_symlinks(self, metadata: PaperMetadata):
        """Create symlinks to full text in all organizational folders"""
        full_text_source = self.base_dir / "full_text" / f"{metadata.arxiv_id}.txt"
        
        if not full_text_source.exists():
            return
        
        # Create symlinks in category folders
        for category in metadata.categories:
            cat_dir = self.base_dir / "by_category" / category
            full_text_link = cat_dir / f"{metadata.arxiv_id}_fulltext.txt"
            self._create_symlink_or_copy(full_text_source, full_text_link, cat_dir)
        
        # Create symlink in date folder
        date_str = self.extract_date_from_id(metadata.arxiv_id)
        date_dir = self.base_dir / "by_date" / date_str
        full_text_link = date_dir / f"{metadata.arxiv_id}_fulltext.txt"
        self._create_symlink_or_copy(full_text_source, full_text_link, date_dir)
        
        # Create symlinks in author folders
        for author in metadata.authors:
            clean_author = self.clean_filename(author)
            author_dir = self.base_dir / "by_author" / clean_author
            full_text_link = author_dir / f"{metadata.arxiv_id}_fulltext.txt"
            self._create_symlink_or_copy(full_text_source, full_text_link, author_dir)
    
    def _create_symlink_or_copy(self, source_path, link_path, link_dir):
        """Helper method to create symlink or copy file as fallback"""
        try:
            if not link_path.exists():
                rel_path = os.path.relpath(source_path, link_dir)
                os.symlink(rel_path, link_path)
        except OSError:
            # Fallback: copy file if symlinks aren't supported
            import shutil
            shutil.copy2(source_path, link_path)
    
    def collect_papers(self, 
                      max_results: int = 1000,
                      start_date: str = "2024-01-01",
                      end_date: str | None = None,
                      categories: List[str] | None = None):
        """
        Collect papers from arXiv API with robust pagination handling
        
        Args:
            max_results: Maximum number of papers to collect
            start_date: Start date in YYYY-MM-DD format
            end_date: End date in YYYY-MM-DD format (default: today)
            categories: Specific categories to focus on (default: all)
        """
        if end_date is None:
            end_date = datetime.now().strftime("%Y-%m-%d")
        
        # Convert dates to arXiv format (YYYYMMDDHHMM)
        start_arxiv = start_date.replace("-", "") + "0000"
        end_arxiv = end_date.replace("-", "") + "2359"
        
        # Build query
        date_query = f"submittedDate:[{start_arxiv} TO {end_arxiv}]"
        
        if categories:
            cat_query = " OR ".join([f"cat:{cat}" for cat in categories])
            query = f"({cat_query}) AND {date_query}"
        else:
            query = date_query
        
        print(f"Searching for papers with query: {query}")
        print(f"Max results: {max_results}")
        
        # Use smaller batch sizes to avoid pagination issues
        batch_size = min(100, max_results)  # arXiv API works best with batches of 100 or less
        papers_processed = 0
        successful_downloads = 0  # Track papers with successful full text downloads
        
        while successful_downloads < max_results:
            # When downloading source, we may need more papers since some downloads fail
            if self.download_source:
                current_batch_size = min(batch_size * 2, max_results - successful_downloads + 50)  # Fetch extra to account for failures
            else:
                current_batch_size = min(batch_size, max_results - successful_downloads)
            
            print(f"\nFetching batch: {papers_processed + 1}-{papers_processed + current_batch_size}")
            if self.download_source:
                print(f"Successful downloads so far: {successful_downloads}/{max_results}")
            
            # Create search for this batch
            search = arxiv.Search(
                query=query,
                max_results=current_batch_size,
                sort_by=arxiv.SortCriterion.SubmittedDate,
                sort_order=arxiv.SortOrder.Descending
            )
            
            # Manually set the start parameter for pagination
            if papers_processed > 0:
                # For subsequent batches, we need to handle pagination differently
                # Use the last paper's date to create a new query
                if hasattr(self, '_last_paper_date'):
                    # Create a new query with updated date range
                    last_date_str = self._last_paper_date.strftime("%Y%m%d%H%M")
                    date_query = f"submittedDate:[{start_arxiv} TO {last_date_str}]"
                    
                    if categories:
                        query = f"({cat_query}) AND {date_query}"
                    else:
                        query = date_query
                    
                    search = arxiv.Search(
                        query=query,
                        max_results=current_batch_size,
                        sort_by=arxiv.SortCriterion.SubmittedDate,
                        sort_order=arxiv.SortOrder.Descending
                    )
            
            batch_count = 0
            try:
                for paper in search.results():
                    if successful_downloads >= max_results:
                        break
                        
                    papers_processed += 1
                    print(f"Processing paper {papers_processed}: {paper.entry_id}")
                    
                    try:
                        # Save metadata
                        metadata = self.save_paper_metadata(paper)
                        
                        # Track the last paper's date for pagination
                        self._last_paper_date = paper.published
                        
                        # Always create directories (needed for symlinks)
                        self.organize_by_category(metadata)
                        self.organize_by_date(metadata)
                        self.organize_by_author(metadata)
                        
                        # Check if we should count this paper
                        should_count = True
                        
                        # Download and save source text if enabled
                        if self.download_source:
                            print(f"  Downloading source text for {metadata.arxiv_id}...")
                            success = self.download_source_text(paper, metadata)
                            if success:
                                print(f"  ✓ Source text downloaded successfully")
                                # Create symlinks to full text in organizational folders
                                self.create_fulltext_symlinks(metadata)
                            else:
                                print(f"  ✗ Source text download failed - paper will not count towards limit")
                                should_count = False
                        
                        if should_count:
                            self.papers_collected += 1
                            successful_downloads += 1
                            batch_count += 1
                            print(f"  ✓ Paper counted ({successful_downloads}/{max_results})")
                        
                        # Respect rate limits
                        time.sleep(self.delay)
                            
                    except Exception as e:
                        error_msg = f"Error processing paper {paper.entry_id}: {str(e)}"
                        print(error_msg)
                        self.errors.append(error_msg)
                        continue
                
                # If we didn't get any successful papers in this batch, break to avoid infinite loop
                if batch_count == 0:
                    print(f"No more papers available or no successful downloads. Stopping at {successful_downloads} papers.")
                    break
                    
            except arxiv.UnexpectedEmptyPageError as e:
                print(f"Hit pagination limit at {papers_processed} papers processed. This is a known arXiv API limitation.")
                print(f"Successfully collected {successful_downloads} papers with full text before hitting the limit.")
                break
            except Exception as e:
                print(f"Error fetching batch starting at {papers_processed}: {str(e)}")
                # Try to continue with smaller batches
                if batch_size > 10:
                    batch_size = max(10, batch_size // 2)
                    print(f"Reducing batch size to {batch_size} and continuing...")
                    continue
                else:
                    print("Failed even with small batch size. Stopping collection.")
                    break
        
        print(f"\nCollection complete!")
        print(f"Papers processed: {papers_processed}")
        print(f"Papers collected: {self.papers_collected}")
        if self.download_source:
            print(f"Papers with successful full text downloads: {successful_downloads}")
        print(f"Errors: {len(self.errors)}")
        print(f"Dataset saved to: {self.base_dir}")
        
        if self.papers_collected < max_results:
            print(f"Note: Only collected {self.papers_collected} papers out of {max_results} requested due to API limitations or download failures.")


    def collect_papers_by_date_chunks(self, 
                                     max_results: int = 1000,
                                     start_date: str = "2024-01-01",
                                     end_date: str | None = None,
                                     categories: List[str] | None = None,
                                     chunk_days: int = 30):
        """
        Alternative collection method that splits the date range into chunks to avoid pagination issues
        
        Args:
            max_results: Maximum number of papers to collect
            start_date: Start date in YYYY-MM-DD format
            end_date: End date in YYYY-MM-DD format (default: today)
            categories: Specific categories to focus on (default: all)
            chunk_days: Number of days per chunk (smaller = more API calls but avoids pagination)
        """
        if end_date is None:
            end_date = datetime.now().strftime("%Y-%m-%d")
        
        start_dt = datetime.strptime(start_date, "%Y-%m-%d")
        end_dt = datetime.strptime(end_date, "%Y-%m-%d")
        
        print(f"Collecting papers from {start_date} to {end_date}")
        print(f"Using date chunks of {chunk_days} days to avoid pagination issues")
        print(f"Max results: {max_results}")
        
        current_dt = end_dt  # Start from most recent and go backwards
        papers_per_chunk = min(100, max_results // 10)  # Reasonable chunk size
        papers_processed = 0  # Track total papers processed
        successful_downloads = 0  # Track papers with successful full text downloads
        
        while current_dt >= start_dt and successful_downloads < max_results:
            chunk_end = current_dt
            chunk_start = max(start_dt, current_dt - timedelta(days=chunk_days))
            
            print(f"\nProcessing chunk: {chunk_start.strftime('%Y-%m-%d')} to {chunk_end.strftime('%Y-%m-%d')}")
            if self.download_source:
                print(f"Successful downloads so far: {successful_downloads}/{max_results}")
            
            # Convert to arXiv format
            start_arxiv = chunk_start.strftime("%Y%m%d") + "0000"
            end_arxiv = chunk_end.strftime("%Y%m%d") + "2359"
            
            # Build query for this chunk
            date_query = f"submittedDate:[{start_arxiv} TO {end_arxiv}]"
            
            if categories:
                cat_query = " OR ".join([f"cat:{cat}" for cat in categories])
                query = f"({cat_query}) AND {date_query}"
            else:
                query = date_query
            
            # Create search for this chunk
            remaining_papers = max_results - successful_downloads
            # When downloading source, we may need more papers since some downloads fail
            if self.download_source:
                chunk_max = min(papers_per_chunk * 2, remaining_papers + 50)  # Fetch extra to account for failures
            else:
                chunk_max = min(papers_per_chunk, remaining_papers)
            
            search = arxiv.Search(
                query=query,
                max_results=chunk_max,
                sort_by=arxiv.SortCriterion.SubmittedDate,
                sort_order=arxiv.SortOrder.Descending
            )
            
            chunk_count = 0
            try:
                for paper in search.results():
                    if successful_downloads >= max_results:
                        break
                        
                    papers_processed += 1
                    print(f"Processing paper {papers_processed}: {paper.entry_id}")
                    
                    try:
                        # Save metadata
                        metadata = self.save_paper_metadata(paper)
                        
                        # Always create directories (needed for symlinks)
                        self.organize_by_category(metadata)
                        self.organize_by_date(metadata)
                        self.organize_by_author(metadata)
                        
                        # Check if we should count this paper
                        should_count = True
                        
                        # Download and save source text if enabled
                        if self.download_source:
                            print(f"  Downloading source text for {metadata.arxiv_id}...")
                            success = self.download_source_text(paper, metadata)
                            if success:
                                print(f"  ✓ Source text downloaded successfully")
                                # Create symlinks to full text in organizational folders
                                self.create_fulltext_symlinks(metadata)
                            else:
                                print(f"  ✗ Source text download failed - paper will not count towards limit")
                                should_count = False
                        
                        if should_count:
                            self.papers_collected += 1
                            successful_downloads += 1
                            chunk_count += 1
                            print(f"  ✓ Paper counted ({successful_downloads}/{max_results})")
                        
                        # Respect rate limits
                        time.sleep(self.delay)
                            
                    except Exception as e:
                        error_msg = f"Error processing paper {paper.entry_id}: {str(e)}"
                        print(error_msg)
                        self.errors.append(error_msg)
                        continue
                
                print(f"  Collected {chunk_count} papers from this chunk")
                
            except Exception as e:
                print(f"Error processing chunk {chunk_start} to {chunk_end}: {str(e)}")
                self.errors.append(f"Chunk error: {str(e)}")
            
            # Move to next chunk (going backwards in time)
            current_dt = chunk_start - timedelta(days=1)
        
        print(f"\nCollection complete!")
        print(f"Papers processed: {papers_processed}")
        print(f"Papers collected: {self.papers_collected}")
        if self.download_source:
            print(f"Papers with successful full text downloads: {successful_downloads}")
        print(f"Errors: {len(self.errors)}")
        print(f"Dataset saved to: {self.base_dir}")


def main():
    """Create a dataset of papers from arXiv"""
    
    # Choose collection method based on size
    use_chunked_method = True  # Set to True to use the more reliable chunked method
    
    # Collect recent CS and Math papers (good for testing)
    # Note: Due to arXiv API limitations, large collections may hit pagination limits
    for num_papers in [1000, ]: 
        print(f"\n{'='*60}")
        print(f"Building dataset with {num_papers} papers")
        print(f"{'='*60}")
        
        try:
            builder = ArxivDatasetBuilder(
                base_dir=f"arxiv_dataset_{num_papers}_papers",
                download_source=True 
            )

            if use_chunked_method or num_papers > 100:
                # Use the chunked method for larger datasets or when specified
                print("Using date-chunked collection method to avoid pagination issues")
                builder.collect_papers_by_date_chunks(
                    max_results=num_papers,
                    start_date="2024-01-01",
                    end_date="2025-08-30",
                    categories=["cs.AI", "cs.LG", "cs.CL", "math.CO"],  # AI/ML focus
                    chunk_days=15  # Smaller chunks for better reliability
                )
            else:
                # Use the original method for smaller datasets
                print("Using standard collection method")
                builder.collect_papers(
                    max_results=num_papers,
                    start_date="2024-01-01",
                    end_date="2025-08-30",
                    categories=["cs.AI", "cs.LG", "cs.CL", "math.CO"]  # AI/ML focus
                )
            
            print(f"✓ Successfully completed dataset with {builder.papers_collected} papers")
            
        except Exception as e:
            print(f"✗ Failed to build dataset with {num_papers} papers: {str(e)}")
            print("Continuing with next dataset size...")
            continue


if __name__ == "__main__":
    main()