//
//  EditingTable.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/9/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import SwiftUI

fileprivate enum EditingSection: String, Hashable {
    case editor = "Editor"
    case media = "Media"
}

struct EditingTable: View {
    @EnvironmentObject var environment: MonocurlEnvironment
    @StateObject private var state: EditingSceneCache
    
    @State private var section: String = EditingSection.editor.rawValue
    @Binding private var url: URL?
    
    init(_ ref : UnsafeMutablePointer<raw_scene_model>, url: Binding<URL?>) {
        self._state = StateObject(wrappedValue: EditingSceneCache(ref));
        self._url = url
    }
    
    var body: some View {
        VStack(spacing: 0) {
            SectionView(section: $section, url: $url, sections: [EditingSection.editor.rawValue, EditingSection.media.rawValue])
            
            switch (section) {
            case EditingSection.editor.rawValue:
                EditingScene(state: state).tabItem {
                    Text("Editor")
                }
            case EditingSection.media.rawValue:
                Media(state: state).tabItem {
                    Text("Media")
                }
            default:
                Text("Unknown Editor Tab")
            }
        }
        .frame(minWidth: self.environment.inPresentationMode ? 0 : 350,
               idealWidth: self.environment.inPresentationMode ? 0 : 450,
               maxWidth: self.environment.inPresentationMode ? 0 : .infinity, maxHeight: .infinity)
    }
}
