use std::hash::Hasher;
use std::fmt::{Display, Formatter};
use std::io::{Read, Write, Error, ErrorKind};

#[derive(Clone, Copy, Debug)]
pub enum PostprocessError {
	LiteralEol,
	LiteralEof,
	LiteralTooLong,
	EscapeEof,
	MarkerNotFound,
	InvalidLiteral,
	TokenTooLong,
}

const BUF_SIZE: usize = 0x10000;
				
impl Display for PostprocessError {
	fn fmt(&self, f: &mut Formatter) -> Result<(), ::std::fmt::Error> {
		match self {
			&PostprocessError::LiteralEol => write!(f, "unexpected end of line in literal"),
			&PostprocessError::LiteralEof => write!(f, "unexpected end of stream in literal"),
			&PostprocessError::LiteralTooLong => write!(f, "literal too long"),
			&PostprocessError::EscapeEof => write!(f, "unexpected end of escape sequence"),
			&PostprocessError::MarkerNotFound => write!(f, "can't find precompiled header marker in preprocessed file"),
			&PostprocessError::InvalidLiteral => write!(f, "can't create string from literal"),
			&PostprocessError::TokenTooLong => write!(f, "token too long"),
		}
	}
}

impl ::std::error::Error for PostprocessError {
	fn description(&self) -> &str {
		match self {
			&PostprocessError::LiteralEol => "unexpected end of line in literal",
			&PostprocessError::LiteralEof => "unexpected end of stream in literal",
			&PostprocessError::LiteralTooLong => "literal too long",
			&PostprocessError::EscapeEof => "unexpected end of escape sequence",
			&PostprocessError::MarkerNotFound => "can't find precompiled header marker in preprocessed file",
			&PostprocessError::InvalidLiteral => "can't create string from literal",
			&PostprocessError::TokenTooLong => "token too long",
		}
	}

	fn cause(&self) -> Option<&::std::error::Error> {
		None
	}
}

#[derive(PartialEq)]
#[derive(Hash)]
#[derive(Eq)]
#[derive(Clone)]
#[derive(Debug)]
pub enum Include<T> {
	Quoted(T),
	Angle(T),
}

pub fn filter_preprocessed(reader: &mut Read, writer: &mut Write, marker: &Option<String>, keep_headers: bool) -> Result<(), Error> {
	let mut state = ScannerState {
		buf_data: [0; BUF_SIZE],
		buf_read: 0,
		buf_copy: 0,
		buf_size: 0,

		reader: reader,
		writer: writer,

		keep_headers: keep_headers,
		marker: None,
		utf8: false,
		header_found: false,
		entry_file: None,
		done: false,
	};
	try! (state.parse_bom());
	state.marker = match marker.as_ref() {
		Some(ref v) => {
			match state.utf8 {
				true => Some(Vec::from(v.as_bytes())),
				false => Some(try! (string_to_local_bytes(v.replace("\\", "/")))),
			}
		},
		None => None,
	};
	while state.buf_size != 0 {
		try!(state.parse_line());
		if state.done {
			return state.copy_to_end();
		}
	}
	Err(Error::new(ErrorKind::InvalidInput, PostprocessError::MarkerNotFound))
}

struct ScannerState<'a> {
	buf_data: [u8; BUF_SIZE],
	buf_read: usize,
	buf_copy: usize,
	buf_size: usize,

	reader: &'a mut Read,
	writer: &'a mut Write,

	keep_headers: bool,
	marker: Option<Vec<u8>>,
	
	utf8: bool,
	header_found: bool,
	entry_file: Option<Vec<u8>>,
	done: bool,
}

impl <'a> ScannerState<'a> {
	fn write(&mut self, data: &[u8]) -> Result<(), Error> {
		try! (self.flush());
		try! (self.writer.write(data));
		Ok(())
	}

	#[inline(always)]
	fn peek(&mut self) -> Result<Option<u8>, Error> {
		if self.buf_read == self.buf_size {
			try! (self.read());
		}
		if self.buf_size == 0 {
			return Ok(None)
		}
		Ok(Some(self.buf_data[self.buf_read]))
	}

	#[inline(always)]
	fn next(&mut self) {
		assert! (self.buf_read < self.buf_size);
		self.buf_read += 1;
	}

	#[inline(always)]
	fn read(&mut self) -> Result<usize, Error> {
		if self.buf_read == self.buf_size {
			try! (self.flush());
			self.buf_read = 0;
			self.buf_copy = 0;
			self.buf_size = try! (self.reader.read(&mut self.buf_data));
		}
		Ok(self.buf_size)
	}

	fn copy_to_end(&mut self) -> Result<(), Error> {
		try!(self.writer.write(&self.buf_data[self.buf_copy..self.buf_size]));
		self.buf_copy = 0;
		self.buf_size = 0;
		loop {
			match try! (self.reader.read(&mut self.buf_data)) {
				0 => {
					return Ok(());
				}
				size => {
					try! (self.writer.write(&self.buf_data[0..size]));
				}
			}
		}
	}


	fn flush(&mut self) -> Result<(), Error> {
		if self.buf_copy != self.buf_read {
			if self.keep_headers {
				try! (self.writer.write(&self.buf_data[self.buf_copy..self.buf_read]));
			}
			self.buf_copy = self.buf_read;
		}
		Ok(())
	}

	fn parse_bom(&mut self) -> Result<(), Error> {
		let bom: [u8; 3] = [0xEF, 0xBB, 0xBF];
		for bom_char in bom.iter() {
			match try! (self.peek()) {
				Some(c) if c == *bom_char => {
					self.next();
				}
				Some(_) => {return Ok(());},
				None => {return Ok(());},
			};
		}
		self.utf8 = true;
		Ok(())
	}

	fn parse_line(&mut self) -> Result<(), Error> {
		try! (self.parse_spaces());
		match try!(self.peek()) {
			Some(b'#') => {
				self.next();
				self.parse_directive()
			}
			Some(_) => {
				try! (self.next_line());
				Ok(())
			}
			None => Ok(()),
		}
	}

	fn next_line(&mut self) -> Result<&'static [u8], Error> {
		loop {
			for i in self.buf_read..self.buf_size {
				let c = self.buf_data[i];
				if c | (b'\n' | b'\r') != (b'\n' | b'\r') {
					continue;
				}
				if c == b'\r' {
					self.buf_read = i + 1;
					if self.buf_read == self.buf_size {
						if try! (self.read()) == 0 {
							return Ok(b"\r");
						}

					}
					if self.buf_data[self.buf_read] == b'\n' {
						self.buf_read += 1;
						return Ok(b"\r\n");
					}
					// end-of-line ::= newline | carriage-return | carriage-return newline
					return Ok(b"\r");

				}
				if c == b'\n' {
					// end-of-line ::= newline | carriage-return | carriage-return newline
					self.buf_read = i + 1;
					return Ok(b"\n");
				}
			}
			self.buf_read = self.buf_size;
			if try! (self.read()) == 0 {
				return Ok(b"");
			}
		}
	}

	fn parse_directive(&mut self) -> Result<(), Error> {
		try!(self.parse_spaces());
		let mut token = [0; 0x10];
		match &try!(self.parse_token(&mut token))[..] {
			b"line" => self.parse_directive_line(),
			b"pragma" => self.parse_directive_pragma(),
			_ => {
				try! (self.next_line());
				Ok(())
			}
		}	
	}

	fn parse_directive_line(&mut self) -> Result<(), Error> {
		let mut line_token = [0; 0x10];
		let mut file_token = [0; 0x400];
		let mut file_raw = [0; 0x400];
		try!(self.parse_spaces());
		let line = try!(self.parse_token(&mut line_token));
		try!(self.parse_spaces());
		let (file, raw) = try!(self.parse_path(&mut file_token, &mut file_raw));
		let eol = try!(self.next_line());
		self.entry_file = match self.entry_file.take() {
			Some(path) => {
				if self.header_found && (path == file) {
					self.done = true;
					let mut mark = Vec::with_capacity(0x400);
					try! (mark.write(b"#pragma hdrstop"));
					try! (mark.write(&eol));
					try! (mark.write(b"#line "));
					try! (mark.write(&line));
					try! (mark.write(b" "));
					try! (mark.write(&raw));
					try! (mark.write(&eol));
					try! (self.write(&mark));
				}
				match &self.marker {
					&Some(ref path) => {
						if is_subpath(&file, &path) {
							self.header_found = true;
						}
					}
					&None => {}
				}
				Some(path)
			}
			None => Some(Vec::from(file))
		};
		Ok(())
	}

	fn parse_directive_pragma(&mut self) -> Result<(), Error> {
		try!(self.parse_spaces());
		let mut token = [0; 0x20];
		match &try!(self.parse_token(&mut token))[..] {
			b"hdrstop" => {
				if !self.keep_headers {
					try! (self.write(b"#pragma hdrstop"));
				}
				self.done = true;
			},
			_ => {
				try! (self.next_line());
			}
		}
		Ok(())
	}

	fn parse_escape(&mut self) -> Result<u8, Error> {
		self.next();
		match try! (self.peek()) {
			Some(c) => {
				self.next();
				match c {
					b'n' => Ok(b'\n'),
					b'r' => Ok(b'\r'),
					b't' => Ok(b'\t'),
					c => Ok(c)
				}
			}
			None => {
				Err(Error::new(ErrorKind::InvalidInput, PostprocessError::EscapeEof))
			}
		}
	}

	fn parse_spaces(&mut self) -> Result<(), Error> {
		loop {
			assert! (self.buf_size <= self.buf_data.len());
			while self.buf_read != self.buf_size {
				match self.buf_data[self.buf_read] {
					// non-nl-white-space ::= a blank, tab, or formfeed character
					b' ' | b'\t' | b'\x0C' => {
						self.next();
					}
					_ => {
						return Ok(());
					}
				}
			}
			if try! (self.read()) == 0 {
				return Ok(());
			}
		}
	}

	fn parse_token<'b>(&mut self, token: &'b mut [u8]) -> Result<&'b [u8], Error> {
		let mut offset: usize = 0;
		loop {
			assert! (self.buf_size <= self.buf_data.len());
			while self.buf_read != self.buf_size {
				let c: u8 = self.buf_data[self.buf_read];
				match c {
					// end-of-line ::= newline | carriage-return | carriage-return newline
					b'a'...b'z' | b'A'...b'Z' | b'0'...b'9' | b'_' => {
						if offset == token.len() {
							return Err(Error::new(ErrorKind::InvalidInput, PostprocessError::TokenTooLong))
						}
						token[offset] = c;
						offset += 1;
					}
					_ => {
						return Ok(&token[0..offset]);
					}
				}
				self.next();
			}
			if try! (self.read()) == 0 {
				return Ok(token);
			}
		}
	}

	fn parse_path<'t, 'r>(&mut self, token: &'t mut [u8], raw: &'r mut [u8]) -> Result<(&'t [u8], &'r [u8]), Error> {
		let quote = try! (self.peek()).unwrap();
		raw[0] = quote;
		self.next();
		let mut token_offset = 0;
		let mut raw_offset = 1;
		loop {
			assert! (self.buf_size <= self.buf_data.len());
			while self.buf_read != self.buf_size {
				let c: u8 = self.buf_data[self.buf_read];
				match c {
					// end-of-line ::= newline | carriage-return | carriage-return newline
					b'\n' | b'\r' => {
						return Err(Error::new(ErrorKind::InvalidInput, PostprocessError::LiteralEol));
					}
					b'\\' => {
						raw[raw_offset+0] = b'\\';
						raw[raw_offset+1] = c;
						raw_offset += 2;
						token[token_offset] = match try!(self.parse_escape()) {
							b'\\' => b'/',
							v => v,
						};
						token_offset += 1;
					}
					c => {
						self.next();
						raw[raw_offset] = c;
						raw_offset += 1;
						if c == quote {
							return Ok((&token[..token_offset], &raw[..raw_offset]));
						}
						token[token_offset] = c;
						token_offset += 1;
					}
				}
				if (raw_offset >= raw.len() - 2) || (token_offset >= token.len() - 1) {
					return Err(Error::new(ErrorKind::InvalidInput, PostprocessError::LiteralTooLong))
				}
			}
			if try! (self.read()) == 0 {
					return Err(Error::new(ErrorKind::InvalidInput, PostprocessError::LiteralEof));
			}
		}
	}
}

fn is_subpath(parent: &[u8], child: &[u8]) -> bool {
	if parent == child {
		return true;
	}
	parent.ends_with(child) && (parent[parent.len() - child.len() - 1] == b'/')
}

fn string_to_local_bytes(s: String) -> Result<Vec<u8>, Error> {
	#[cfg(unix)]
	fn string_to_local_bytes_inner(s: String) -> Result<Vec<u8>, Error> {
		use std::ffi::OsString;

		match OsString::from(s).to_bytes() {
			Some(v) => Ok(Vec::from(v)),
			None => Err(Error::new(ErrorKind::InvalidInput, PostprocessError::InvalidLiteral)),
		}
	}

	#[cfg(windows)]
	fn string_to_local_bytes_inner(s: String) -> Result<Vec<u8>, Error> {
		extern crate winapi;
		extern crate kernel32;

		use std::ptr;
		use std::iter::FromIterator;

		const WC_COMPOSITECHECK: winapi::DWORD = 0x00000200; // use composite chars

		// Empty string
		if s.len() == 0 {
			return Ok(Vec::new());
		}
		unsafe {
			let wstr: Vec<u16> = Vec::from_iter(s.utf16_units());
			// Get length of ANSI string
			let len = kernel32::WideCharToMultiByte(winapi::CP_ACP, WC_COMPOSITECHECK, wstr.as_ptr(), wstr.len() as i32, ptr::null_mut(), 0, ptr::null(), ptr::null_mut());
			if len <= 0 {
				return Err(Error::new(ErrorKind::InvalidInput, PostprocessError::InvalidLiteral));
			}
			// Convert UTF-16 to ANSI
			let mut astr: Vec<u8> = Vec::with_capacity(len as usize);
			astr.set_len(len as usize);
			match kernel32::WideCharToMultiByte(winapi::CP_ACP, WC_COMPOSITECHECK, wstr.as_ptr(), wstr.len() as i32, astr.as_mut_ptr() as winapi::LPSTR, len, ptr::null(), ptr::null_mut()) {
				l if (l as usize) == astr.len() => Ok(astr),
				l if l > 0 => Ok(Vec::from(&astr[0..(l as usize)])),
				_ => Err(Error::new(ErrorKind::InvalidInput, PostprocessError::InvalidLiteral)),
			}
		}
	}

	string_to_local_bytes_inner(s)
}

#[cfg(test)]
mod test {
	extern crate test;

	use std::io::{Read, Write, Cursor};
	use std::fs::File;
	use self::test::Bencher;

	fn check_filter_pass(original: &str, expected: &str, marker: &Option<String>, keep_headers: bool, eol: &str) {
		let mut writer: Vec<u8> = Vec::new();
		let mut stream: Vec<u8> = Vec::new();
		stream.write(&original.replace("\n", eol).as_bytes()[..]).unwrap();
		match super::filter_preprocessed(&mut Cursor::new(stream), &mut writer, marker, keep_headers) {
			Ok(_) => {assert_eq! (String::from_utf8_lossy(&writer), expected.replace("\n", eol))}
			Err(e) => {panic! (e);}
		}
	}

	fn check_filter(original: &str, expected: &str, marker: Option<String>, keep_headers: bool) {
		check_filter_pass(original, expected, &marker, keep_headers, "\n");
		check_filter_pass(original, expected, &marker, keep_headers, "\r\n");
		check_filter_pass(original, expected, &marker, keep_headers, "\r");
	}

	#[test]
	fn test_filter_precompiled_keep() {
		check_filter(
r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#,
r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"
#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, Some("sample header.h".to_string()), true)
	}

	#[test]
	fn test_filter_precompiled_remove() {
		check_filter(
r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello1();
void hello2();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, 
r#"#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, Some("sample header.h".to_string()), false);
	}

	#[test]
	fn test_filter_precompiled_hdrstop() {
		check_filter(
r#"#line 1 "sample.cpp"
 #line 1 "e:/work/octobuild/test_cl/sample header.h"
void hello();
# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#,
r#"#pragma hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, None, false);
	}

	#[test]
	fn test_filter_precompiled_hdrstop_keep() {
		check_filter(
r#"#line 1 "sample.cpp"
 #line 1 "e:/work/octobuild/test_cl/sample header.h"
void hello();
# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#,
r#"#line 1 "sample.cpp"
 #line 1 "e:/work/octobuild/test_cl/sample header.h"
void hello();
# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, None, true);
	}

	#[test]
	fn test_filter_precompiled_winpath() {
		check_filter(
r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#,
r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"
#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, Some("e:\\work\\octobuild\\test_cl\\sample header.h".to_string()), true);
	}

	fn bench_filter(b: &mut Bencher, path: &str, marker: Option<String>, keep_headers: bool) {
		let mut source = Vec::new();
		File::open(path).unwrap().read_to_end(&mut source).unwrap();
		b.iter(|| {
			let mut result = Vec::with_capacity(source.len());
			super::filter_preprocessed(&mut Cursor::new(source.clone()), &mut result, &marker, keep_headers).unwrap();
			result
		});
	}
	
	#[bench]
	fn bench_check_filter(b: &mut Bencher) {
		bench_filter(b, "tests/filter_preprocessed.i", Some("c:\\bozaro\\github\\octobuild\\test_cl\\sample.h".to_string()), false)
	}

	/**
	 * Test for checking converting ANSI to Unicode characters on Ms Windows.
	 * Since the test is dependent on the environment, checked only Latin characters.
	 */
	#[test]
	fn test_string_to_local_bytes() {
		let s = "test\0data";
		assert_eq!(super::string_to_local_bytes(s.to_string()).unwrap(), s.to_string().into_bytes());
	}
}
