//
//  monocurlTests.swift
//  monocurlTests
//
//  Created by Manu Bhat on 7/13/20.
//

import XCTest

fileprivate var testCase: XCTestCase!

func reportFailure(description: UnsafePointer<CChar>!, file: UnsafePointer<CChar>!, line: Int) {
    testCase
        .record(
            XCTIssue(type: .assertionFailure, compactDescription: String(cString: description), sourceCodeContext: XCTSourceCodeContext(location: XCTSourceCodeLocation(filePath: String(cString: file), lineNumber: line)))
        )
}

func reportSuccess(description: UnsafePointer<CChar>!, file: UnsafePointer<CChar>!, line: Int) {
    
}

extension String {
    var utf8CStringPointer: UnsafePointer<CChar> {
        let cString = self.utf8CString;
        let pointer: UnsafeMutablePointer<CChar> = .allocate(capacity: cString.count)
        
        cString.withContiguousStorageIfAvailable { source in
            let _ = memcpy(pointer, source.baseAddress, source.count);
        }
        
        return UnsafePointer(pointer);
    }
}

func path(for url: URL) -> String {
    if #available(macOS 13.0, *) {
        return url.path(percentEncoded: false)
    } else {
        return url.path
    }
}

//base 64 encoded
func bookmark(for url: URL) -> Data! {
    let url = url as NSURL
    do {
        url.startAccessingSecurityScopedResource()
        let data = try url.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil);
        url.stopAccessingSecurityScopedResource()
        return data;
    } catch let error {
        NSLog("Error creating bookmark! \(error)");
    }
    
    return nil;
}


func translateBookmark(path: UnsafePointer<CChar>!) -> UnsafePointer<CChar>! {
    let base64 = String(cString: path);
    guard let datautf = base64.data(using: .utf8), let dataRaw = Data(base64Encoded: datautf) else {
        NSLog("Could not convert bookmark to data")
        return nil
    }
    guard let url = try? NSURL(resolvingBookmarkData: dataRaw, options: .withSecurityScope, relativeTo: nil, bookmarkDataIsStale: nil) else {
        NSLog("Could not convert data to NSURL")
        return nil;
    }
   
    url.startAccessingSecurityScopedResource()
    
    return Monocurl_Tests.path(for: url as URL).utf8CStringPointer
}

class MonocurlTests: XCTestCase {

    override func setUpWithError() throws {
        report_failure = reportFailure;
        report_success = reportSuccess;
        testCase = self
        
        path_translation = translateBookmark(path:);
        if let url = Bundle.main.url(forResource: "libmc", withExtension: "mcf") {
            std_lib_path = path(for: url).utf8CStringPointer
        }
        if let url = Bundle.main.url(forResource: "mc_default_scene", withExtension: "mcf") {
            default_scene_path = path(for: url).utf8CStringPointer
        }
    }
    
    func testC() {
        measure {
            cengine_tests_run();
        }
    }

    override func tearDownWithError() throws {
        
    }
}
