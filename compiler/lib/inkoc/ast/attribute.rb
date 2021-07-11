# frozen_string_literal: true

module Inkoc
  module AST
    class Attribute
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :location
      attr_accessor :variable_state

      # name - The name of the attribute.
      # location - The SourceLocation of the attribute.
      def initialize(name, location)
        @name = name
        @location = location
        @variable_state = nil
      end

      def visitor_method
        :on_attribute
      end

      def define_variable_visitor_method
        :on_define_attribute
      end

      def reassign_variable_visitor_method
        :on_reassign_attribute
      end

      def attribute?
        true
      end
    end
  end
end
