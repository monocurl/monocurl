//
//  SceneEditor.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/9/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import SwiftUI

struct SceneEditorView: View {
    private let environment: MonocurlEnvironment
    
    @State var failed = false
    @State var scene: UnsafeMutablePointer<scene_handle>!
    @Binding private var url: URL?
    
    init(_ url: Binding<URL?>, environment: MonocurlEnvironment) {
        self._url = url;
        self.environment = environment
    }
    
    var body: some View {
        Group {
            if let ref = scene {
                HSplitView {
                    EditingTable(ref.pointee.model, url: $url)
                    VSplitView {
                        Viewport(ref.pointee.viewport)
                        Timeline(ref.pointee.timeline);
                    }
                    .frame(minWidth: 500, idealWidth: 700, maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            else if failed {
                Text("Unable to initialize viewport. Likely opening malformed file, or the file is of incompatible version with this editor. Try updating monocurl.")
                    .foregroundColor(.red)
            }
            else {
                ProgressView()
            }
        }
        .onTapGesture {
            NSApplication.removeFirstResponder()
        }
        .onAppear {
            self.scene = file_read_sync(path(for: url!).utf8CStringPointer)
            
            if let scene = scene {
                environment.scene = scene;
            }
            else {
                failed = true
            }
        }
        .onDisappear {
            if let scene = scene {
                self.environment.scene = nil;
                
                scene_handle_free(scene);
            }
        }
    }
}
