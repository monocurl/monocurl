//
//  Environment.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/6/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import SwiftUI
import UniformTypeIdentifiers

fileprivate let maxEditors = 6

//https://stackoverflow.com/a/62563773/8697793
extension Array: RawRepresentable where Element: Codable {
    public init?(rawValue: String) {
        guard let data = rawValue.data(using: .utf8),
              let result = try? JSONDecoder().decode([Element].self, from: data) else {
            return nil
        }
        self = result
    }

    public var rawValue: String {
        guard let data = try? JSONEncoder().encode(self),
              let result = String(data: data, encoding: .utf8) else {
            return "[]"
        }
        return result
    }
}

struct Route: Equatable {
    enum RouteType: Equatable {
        case landing
        case editor
    }
    var routeType: RouteType
    
    //for identifying the exact editor
    var subIndex: Int? = nil;
}

//routing and general var transfer
class MonocurlEnvironment: ObservableObject {
    @Published var inPresentationMode = false
    var scene: UnsafeMutablePointer<scene_handle>!
    
    public func togglePresentaiton() {
        if self.scene != nil && self.scene.pointee.timeline != nil {
            self.inPresentationMode = !self.inPresentationMode
            timeline_toggle_presentation_mode(self.scene.pointee.timeline)
            DispatchQueue.main.async {
                if let window = NSApplication.shared.windows.first {
                    window.makeFirstResponder(nil)
                    if window.styleMask.contains(.fullScreen) != self.inPresentationMode {
                        window.toggleFullScreen(nil)
                    }
                }
            }
        }
    }
}

fileprivate func defaultProjects() -> [Data] {
    let fileNames = [
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
        "Monotonic Stacks",
    ]
        .compactMap { Bundle.main.url(forResource: $0, withExtension: "mcf") }
        .compactMap { 
            try? $0.bookmarkData(options: .suitableForBookmarkFile, includingResourceValuesForKeys: nil, relativeTo: nil)
        }
    
    return fileNames
}

class StorageEnvironment: ObservableObject {
    @AppStorage("projects") var bookmarks: [Data] = defaultProjects()
    @Published var urls: [URL];
    
    init() {
        self.urls = [];
        
        var to_delete = IndexSet()
        for (i, bookmark) in  self.bookmarks.enumerated() {
            guard let ret = (try? NSURL(resolvingBookmarkData: bookmark, options: .withSecurityScope, relativeTo: nil, bookmarkDataIsStale: nil)) ?? (try? NSURL(resolvingBookmarkData: bookmark, options: [] , relativeTo: nil, bookmarkDataIsStale: nil)) else {
                to_delete.insert(i)
                continue;
            }
            
            ret.startAccessingSecurityScopedResource()
            self.urls.append(ret as URL)
        }
        
        self.bookmarks.remove(atOffsets: to_delete)
    }
    
    func remove(url: URL) {
        if let index = self.urls.firstIndex(of: url) {
            self.urls.remove(at: index)
            self.bookmarks.remove(at: index)
        }
    }
    
    func makeFirst(url: URL) {
        if let index = self.urls.firstIndex(of: url), index != 0 {
            self.urls.insert(self.urls.remove(at: index), at: 0)
            self.bookmarks.insert(self.bookmarks.remove(at: index), at: 0)
        }
    }

    func addUrl(_ type: UTType, withUrl: (URL) -> ()) {
        let folderPicker = NSSavePanel();
        folderPicker.allowedContentTypes = [type]
        let res = folderPicker.runModal()
        
        if res == .OK {
            withUrl(folderPicker.url!);
            
            if !self.urls.contains(folderPicker.url!), let bookmark = bookmark(for: folderPicker.url!) {
                self.urls.insert(folderPicker.url!, at: 0)
                self.bookmarks.insert(bookmark, at: 0);
            }
        }
    }

    @discardableResult
    func tryImport() -> Data? {
        let folderPicker = NSOpenPanel();
        folderPicker.allowedContentTypes = [AppRoot.sceneType]
        let res = folderPicker.runModal()
        
        if (res == .OK) {
            guard let bookmark = bookmark(for: folderPicker.url!) else {
                return nil;
            }
            
            if !self.urls.contains(folderPicker.url!) {
                self.urls.insert(folderPicker.url!, at: 0)
                self.bookmarks.insert(bookmark, at: 0)
            }
            
            return bookmark;
        }
        
        return nil;
    }
    
    deinit {
        for url in self.urls {
            (url as NSURL).stopAccessingSecurityScopedResource()
        }
    }
}
